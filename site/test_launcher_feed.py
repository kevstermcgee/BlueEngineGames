import copy
import unittest

from launcher_feed import entry


def release(tag, date='2026-10-18T08:00:00Z', slug='example', kind='zip', **extra):
    suffix = '-windows-x64.zip' if kind == 'zip' else '-setup-windows-x64.exe'
    filename = slug + suffix
    return dict(tag_name=tag, name=f'Published {tag}', published_at=date,
                assets=[dict(name=filename, size=1234, digest='sha256:' + 'a' * 64,
                             browser_download_url=f'https://github.com/kevstermcgee/BlueEngineGames/releases/download/{tag}/{filename}')],
                **extra)


def game(current=None, history=(), **fields):
    row = dict(slug='example', name='Example', game_version='1.0', sha256='b' * 64, **fields)
    return entry(row, current or release('current'), {'gallery': [], 'characters': []},
                 None, 'human', '2020-01-01', releases=history)


class PublishedVersions(unittest.TestCase):
    def test_latest_and_older_dates_come_from_published_release_metadata(self):
        current = release('current')
        older = release('older', '2026-10-01T08:00:00Z')
        recent = release('recent', '2026-10-17T08:00:00Z')
        feed = game(current, [older, recent, current, copy.deepcopy(recent)])
        self.assertEqual(feed['created'], '2020-01-01')
        self.assertEqual(feed['released_at'], current['published_at'])
        self.assertEqual([v['release'] for v in feed['versions']], ['current', 'recent', 'older'])
        self.assertEqual([v['latest'] for v in feed['versions']], [True, False, False])
        self.assertEqual(feed['versions'][1]['released_at'], recent['published_at'])
        self.assertEqual(feed['versions'][1]['name'], recent['name'])
        self.assertEqual(feed['versions'][0]['package'], feed['package'])
        self.assertEqual(feed['versions'][0]['package']['sha256'], 'b' * 64)
        self.assertEqual(feed['versions'][1]['package']['sha256'], 'a' * 64)

    def test_dates_stay_unknown_without_a_valid_published_timestamp(self):
        for invalid in (None, '', 'not a date', '2026-10-01', '2026-02-30T00:00:00Z'):
            with self.subTest(date=invalid):
                feed = game(release('current', invalid), [release('unknown', invalid), release('dated')])
                self.assertIsNone(feed['released_at'])
                self.assertIsNone(feed['versions'][0]['released_at'])
                self.assertEqual(feed['versions'][1]['release'], 'dated')
                self.assertIsNone(feed['versions'][2]['released_at'])

    def test_only_published_releases_with_this_games_assets_are_listed(self):
        feed = game(history=[release('draft', draft=True), release('preview', prerelease=True),
                             release('unrelated', slug='other'), release('older')])
        self.assertEqual([v['release'] for v in feed['versions']], ['current', 'older'])

    def test_unpinned_legacy_packages_are_listed_without_an_installable_package(self):
        legacy = release('legacy')
        legacy['assets'][0].pop('digest')
        feed = game(history=[legacy])
        self.assertEqual(feed['versions'][1]['released_at'], legacy['published_at'])
        self.assertIsNone(feed['versions'][1]['package'])

    def test_pinned_installer_is_available_when_a_historical_zip_is_unpinned(self):
        older = release('older')
        older['assets'][0]['digest'] = ''
        older['assets'].extend(release('older', kind='installer')['assets'])
        package = game(history=[older])['versions'][1]['package']
        self.assertEqual(package['kind'], 'installer')
        self.assertTrue(package['url'].endswith('-setup-windows-x64.exe'))

    def test_independent_games_use_their_own_dated_history(self):
        current = release('current', kind='installer')
        previous = release('previous', '2026-10-12T04:56:09Z', kind='installer')
        feed = game(current, [release('bundled-history')], published_release=current,
                    previous_releases=[previous])
        self.assertEqual([v['release'] for v in feed['versions']], ['current', 'previous'])
        self.assertEqual(feed['versions'][1]['released_at'], previous['published_at'])
        self.assertEqual(feed['versions'][1]['package']['kind'], 'installer')


if __name__ == '__main__':
    unittest.main()
