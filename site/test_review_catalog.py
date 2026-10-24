from http.client import IncompleteRead
import io
import json
import socket
import unittest
from unittest.mock import Mock
from urllib.error import HTTPError

import review_catalog as coverage


def response(document):
    return io.BytesIO(json.dumps(document).encode())


def accepted(games, **extra):
    return {"schema_version": 1, "games": [{"slug": slug, "title": title} for slug, title in games.items()],
            "reviewable_games": games, **extra}


def published(games):
    return {"version": 1, "games": [{"slug": slug, "name": title} for slug, title in games.items()]}


class ReviewPublicationCoverage(unittest.TestCase):
    def test_original_missing_independent_and_new_native_games_block_publication_with_names(self):
        original = {f"legacy-{index}": f"Legacy {index}" for index in range(51)}
        newly_published = {"bluecity": "BlueCity", "scatterlock": "Scatterlock", "tetris": "Tetris"}
        opener = Mock()
        opener.open.side_effect = [response(accepted(original)) for _ in range(3)]
        sleep = Mock()
        with self.assertRaisesRegex(coverage.CoverageError, "BlueCity \\(bluecity\\).*Scatterlock \\(scatterlock\\).*Tetris \\(tetris\\).*"):
            coverage.check(published(original | newly_published), opener=opener, sleep=sleep)
        self.assertEqual(opener.open.call_count, 3, "A catalog gap gets only the bounded propagation window")
        self.assertEqual([call.args for call in sleep.call_args_list], [(5,), (15,)])

    def test_all_published_games_pass_with_unpublished_extras_and_different_display_titles(self):
        opener = Mock()
        opener.open.return_value = response(accepted({"bluecity": "Earlier title", "automation-workshop": "Unpublished tool"}))
        result = coverage.check(published({"bluecity": "BlueCity"}), opener=opener)
        self.assertEqual((result["published_games"], result["reviewable_games"]), (1, 2))
        request = opener.open.call_args.args[0]
        self.assertEqual((request.full_url, request.get_method()), (coverage.ENDPOINT, "GET"))
        self.assertEqual(opener.open.call_args.kwargs, {"timeout": coverage.TIMEOUT})
        self.assertFalse(request.has_header("Authorization"))

    def test_creator_draft_presence_does_not_claim_review_eligibility(self):
        opener = Mock()
        document = accepted({})
        document["games"] = [{"slug": "draft", "title": "Unverified creator draft"}]
        opener.open.side_effect = [response(document) for _ in range(3)]
        with self.assertRaisesRegex(coverage.CoverageError, "Draft \\(draft\\)"):
            coverage.check(published({"draft": "Draft"}), opener=opener, sleep=Mock())

    def test_new_publication_can_wait_for_the_receiver_freshness_window(self):
        opener, sleep = Mock(), Mock()
        opener.open.side_effect = [response(accepted({})), response(accepted({"bluecity": "BlueCity"}))]
        self.assertEqual(coverage.check(published({"bluecity": "BlueCity"}), opener=opener, sleep=sleep)["published_games"], 1)
        self.assertEqual(opener.open.call_count, 2)
        sleep.assert_called_once_with(5)

    def test_catalog_propagation_and_transport_failures_share_one_request_budget(self):
        opener, sleep = Mock(), Mock()
        opener.open.side_effect = [response(accepted({})), TimeoutError("timed out"),
                                   response(accepted({"bluecity": "BlueCity"}))]
        self.assertEqual(coverage.check(published({"bluecity": "BlueCity"}), opener=opener, sleep=sleep)["published_games"], 1)
        self.assertEqual(opener.open.call_count, 3)
        self.assertEqual(sleep.call_count, 2)

    def test_deployed_receiver_must_report_valid_explicit_eligibility(self):
        invalid = [None, [], {"schema_version": 2, "games": []},
                   {"schema_version": 1, "games": []},
                   accepted({"../unsafe": "Unsafe"}), accepted({"bluecity": ""})]
        for document in invalid:
            with self.subTest(document=document):
                opener = Mock()
                opener.open.return_value = response(document)
                with self.assertRaises(coverage.CoverageError):
                    coverage.check(published({"bluecity": "BlueCity"}), opener=opener)
                self.assertEqual(opener.open.call_count, 1)
        with self.assertRaisesRegex(coverage.CoverageError, "Deploy the current receiver"):
            coverage.reviewable_games({"schema_version": 1, "games": []})

    def test_transient_http_and_network_failures_retry_then_succeed_using_same_readonly_endpoint(self):
        opener, sleep = Mock(), Mock()
        opener.open.side_effect = [HTTPError(coverage.ENDPOINT, 503, "Unavailable", {}, None),
                                   socket.timeout("timed out"), response(accepted({"bluecity": "BlueCity"}))]
        self.assertEqual(coverage.check(published({"bluecity": "BlueCity"}), opener=opener, sleep=sleep)["published_games"], 1)
        self.assertEqual([call.args for call in sleep.call_args_list], [(5,), (15,)])
        self.assertTrue(all(call.args[0].full_url == coverage.ENDPOINT and call.args[0].get_method() == "GET"
                            for call in opener.open.call_args_list))

    def test_interrupted_response_is_retried_without_a_partial_catalog(self):
        opener, sleep = Mock(), Mock()
        interrupted = Mock()
        interrupted.__enter__ = Mock(return_value=interrupted)
        interrupted.__exit__ = Mock(return_value=False)
        interrupted.read.side_effect = IncompleteRead(b'{"schema_version":1,', 100)
        opener.open.side_effect = [interrupted, response(accepted({"bluecity": "BlueCity"}))]
        self.assertEqual(coverage.check(published({"bluecity": "BlueCity"}), opener=opener, sleep=sleep)["published_games"], 1)
        interrupted.read.assert_called_once_with(coverage.MAX_BODY + 1)
        sleep.assert_called_once_with(5)

    def test_persistent_timeouts_fail_with_bounded_attempts_and_actionable_error(self):
        opener, sleep = Mock(), Mock()
        opener.open.side_effect = TimeoutError("timed out")
        with self.assertRaisesRegex(coverage.CoverageError, "after 3 attempts.*Restore it and rerun publication"):
            coverage.check(published({"bluecity": "BlueCity"}), opener=opener, sleep=sleep)
        self.assertEqual(opener.open.call_count, 3)
        self.assertEqual(sleep.call_count, 2)

    def test_permanent_http_errors_and_redirects_are_not_retried(self):
        for status in (302, 400, 403, 404):
            with self.subTest(status=status):
                opener = Mock()
                opener.open.side_effect = HTTPError(coverage.ENDPOINT, status, "Unavailable", {}, None)
                with self.assertRaisesRegex(coverage.CoverageError, f"HTTP {status}.*before publishing"):
                    coverage.check(published({"bluecity": "BlueCity"}), opener=opener)
                self.assertEqual(opener.open.call_count, 1)
        self.assertIsNone(coverage.NoRedirect().redirect_request(None, None, 302, "Redirect", {}, "https://elsewhere.invalid"))

    def test_malformed_or_oversized_response_cannot_pass_or_retry(self):
        for body in (b"not JSON", b"x" * (coverage.MAX_BODY + 1)):
            with self.subTest(size=len(body)):
                opener = Mock()
                stream = io.BytesIO(body)
                opener.open.return_value = stream
                with self.assertRaises(coverage.CoverageError):
                    coverage.check(published({"bluecity": "BlueCity"}), opener=opener)
                self.assertEqual(opener.open.call_count, 1)

    def test_invalid_generated_catalog_fails_before_contacting_receiver(self):
        for document in (None, {"version": 1, "games": [{"slug": "../unsafe", "name": "Unsafe"}]},
                         {"version": 1, "games": [{"slug": "same", "name": "One"}, {"slug": "same", "name": "Two"}]}):
            opener = Mock()
            with self.assertRaises(coverage.CoverageError):
                coverage.check(document, opener=opener)
            opener.open.assert_not_called()


if __name__ == "__main__":
    unittest.main()
