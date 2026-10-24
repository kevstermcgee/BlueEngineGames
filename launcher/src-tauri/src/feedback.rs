use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    time::{Duration, Instant},
};

#[cfg(not(feature = "feedback-smoke"))]
pub const ENDPOINT: &str = "https://blue-engine.duckdns.org/feedback";
#[cfg(feature = "feedback-smoke")]
pub const ENDPOINT: &str = "http://127.0.0.1:9235/feedback";
const FORMAT: &str = "blueengine-player-feedback-v1";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub format: String,
    pub submission_id: String,
    pub game: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub rating: u8,
    #[serde(default)]
    pub enjoyed: String,
    #[serde(default)]
    pub disliked: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Receipt {
    pub accepted: bool,
    pub submission_id: String,
}

fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
        && value.as_bytes()[14] == b'4'
        && b"89ab".contains(&value.as_bytes()[19])
}

pub fn validate(review: &Review) -> Result<(), String> {
    let text_valid = |value: &str, limit| {
        value.chars().count() <= limit
            && !value
                .chars()
                .any(|c| c.is_control() && !['\n', '\r', '\t'].contains(&c))
    };
    if review.format != FORMAT
        || !valid_id(&review.submission_id)
        || review.game.is_empty()
        || review.game.len() > 80
        || !review
            .game
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || !(1..=10).contains(&review.rating)
        || !text_valid(&review.name, 80)
        || !text_valid(&review.version, 160)
        || !text_valid(&review.enjoyed, 2000)
        || !text_valid(&review.disliked, 2000)
        || (review.enjoyed.trim().is_empty() && review.disliked.trim().is_empty())
    {
        return Err("Check your name, rating and feedback before submitting.".into());
    }
    Ok(())
}

pub fn submit(review: &Review) -> Result<Receipt, String> {
    submit_to(ENDPOINT, review)
}

const CONNECTION_ERROR: &str =
    "Feedback could not be sent. Your draft is still here. Check your connection and try again.";
const UNAVAILABLE_ERROR: &str = "The feedback service is temporarily unavailable. Your draft is still here; please try again shortly.";
const RATE_LIMIT_ERROR: &str =
    "Please wait a few minutes before trying again. Your draft is still here.";
const INVALID_ERROR: &str = "The review was not accepted. Check your rating and written answers before trying again. Your draft is still here.";
const CONFLICT_ERROR: &str = "A previous submission uses this review ID. Edit your draft before submitting again. Your draft is still here.";
const RECEIPT_ERROR: &str = "The feedback receipt could not be verified. Your draft is still here; retrying safely uses the same review ID.";
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];

fn submit_to(endpoint: &str, review: &Review) -> Result<Receipt, String> {
    submit_with_policy(endpoint, review, Duration::from_secs(90), &RETRY_DELAYS)
}

fn submit_with_policy(
    endpoint: &str,
    review: &Review,
    budget: Duration,
    retry_delays: &[Duration],
) -> Result<Receipt, String> {
    validate(review)?;
    let started = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| CONNECTION_ERROR)?;
    // Serialize once so a lost acknowledgement is retried with exactly the same ID and content.
    let body = serde_json::to_vec(review).map_err(|_| INVALID_ERROR)?;
    for attempt in 0..=retry_delays.len() {
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(CONNECTION_ERROR.into());
        }
        let result = submit_attempt(
            &client,
            endpoint,
            review,
            &body,
            remaining.min(Duration::from_secs(60)),
        );
        match result {
            Ok(receipt) => return Ok(receipt),
            Err((message, retry)) => {
                if !retry || attempt == retry_delays.len() {
                    return Err(message.into());
                }
                let remaining = budget.saturating_sub(started.elapsed());
                if remaining <= retry_delays[attempt] {
                    return Err(message.into());
                }
                std::thread::sleep(retry_delays[attempt]);
            }
        }
    }
    unreachable!("each final attempt returns")
}

fn submit_attempt(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    review: &Review,
    body: &[u8],
    timeout: Duration,
) -> Result<Receipt, (&'static str, bool)> {
    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_vec())
        .timeout(timeout)
        .send()
        .map_err(|_| (CONNECTION_ERROR, true))?;
    let status = response.status();
    if !status.is_success() {
        let message = match status.as_u16() {
            429 => RATE_LIMIT_ERROR,
            409 => CONFLICT_ERROR,
            400 | 413 | 422 => INVALID_ERROR,
            _ => UNAVAILABLE_ERROR,
        };
        let retry = matches!(status.as_u16(), 408 | 425 | 500 | 502 | 503 | 504);
        return Err((message, retry));
    }
    let mut body = Vec::new();
    response
        .take(4097)
        .read_to_end(&mut body)
        .map_err(|_| (RECEIPT_ERROR, true))?;
    if body.len() > 4096 {
        return Err((RECEIPT_ERROR, false));
    }
    let receipt: Receipt = serde_json::from_slice(&body).map_err(|_| (RECEIPT_ERROR, true))?;
    if !receipt.accepted || receipt.submission_id != review.submission_id {
        return Err((RECEIPT_ERROR, false));
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};

    fn review() -> Review {
        serde_json::from_value(serde_json::json!({
            "format": FORMAT, "submission_id": "1bf12b5d-1eef-4f23-98e5-6c941b28cb34",
            "game": "pocket-breaker", "rating": 8, "enjoyed": "Good controls 🎮"
        }))
        .unwrap()
    }

    #[test]
    fn optional_name_and_unicode_reviews_need_no_account() {
        let mut review = review();
        assert!(validate(&review).is_ok());
        review.name = "A player 🎮".into();
        review.enjoyed = "🎮".repeat(2000);
        assert!(validate(&review).is_ok());
        review.enjoyed.push('x');
        assert!(validate(&review).is_err());
        review.enjoyed = "Good".into();
        review.name = "x".repeat(81);
        assert!(validate(&review).is_err());
        review.name.clear();
        review.submission_id = "../review".into();
        assert!(validate(&review).is_err());
    }

    fn server(status: &str, body: String) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
        server_sequence(vec![(Some((status.to_string(), body)), Duration::ZERO)])
    }

    fn server_sequence(
        responses: Vec<(Option<(String, String)>, Duration)>,
    ) -> (String, thread::JoinHandle<Vec<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/feedback", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let mut requests = Vec::new();
            for (response, delay) in responses {
                let accept_deadline = Instant::now() + Duration::from_secs(5);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < accept_deadline,
                                "timed out waiting for expected review request"
                            );
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("review fixture accept failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut bytes = [0; 4096];
                    let length = stream.read(&mut bytes).unwrap();
                    assert!(length > 0);
                    request.extend_from_slice(&bytes[..length]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            assert!(!headers.contains("authorization:"));
                            assert!(!headers.contains("cookie:"));
                            assert!(headers.starts_with("post /feedback "));
                            let payload: serde_json::Value =
                                serde_json::from_slice(&request[end + 4..]).unwrap();
                            assert_eq!(payload["name"], "");
                            requests.push(request[end + 4..].to_vec());
                            break;
                        }
                    }
                }
                thread::sleep(delay);
                if let Some((status, body)) = response {
                    // A timeout test deliberately closes before this delayed response.
                    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                }
            }
            requests
        });
        (endpoint, task)
    }

    #[test]
    fn accepts_only_a_matching_published_receipt() {
        for (status, body, accepted) in [
            (
                "200 OK",
                format!(
                    r#"{{"accepted":true,"submission_id":"{}"}}"#,
                    review().submission_id
                ),
                true,
            ),
            (
                "200 OK",
                r#"{"accepted":true,"submission_id":"wrong"}"#.into(),
                false,
            ),
            ("200 OK", r#"{"accepted":false}"#.into(), false),
            ("429 Too Many Requests", "{}".into(), false),
            ("503 Unavailable", "{}".into(), false),
            ("302 Found", "{}".into(), false),
        ] {
            let (url, task) = server(status, body);
            assert_eq!(
                submit_with_policy(&url, &review(), Duration::from_secs(2), &[]).is_ok(),
                accepted
            );
            task.join().unwrap();
        }
    }

    fn receipt_body() -> String {
        format!(
            r#"{{"accepted":true,"submission_id":"{}"}}"#,
            review().submission_id
        )
    }

    #[test]
    fn transient_failures_and_lost_receipts_retry_identical_review_bytes() {
        let (url, task) = server_sequence(vec![
            (None, Duration::ZERO),
            (
                Some(("503 Unavailable".into(), "{}".into())),
                Duration::ZERO,
            ),
            (Some(("200 OK".into(), receipt_body())), Duration::ZERO),
        ]);
        let receipt = submit_with_policy(
            &url,
            &review(),
            Duration::from_secs(2),
            &[Duration::ZERO; 2],
        )
        .unwrap();
        assert!(receipt.accepted);
        let requests = task.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0], requests[1]);
        assert_eq!(requests[1], requests[2]);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[0]).unwrap()["submission_id"],
            review().submission_id
        );
    }

    #[test]
    fn permanent_errors_are_actionable_and_do_not_retry() {
        for (status, message) in [
            ("400 Bad Request", INVALID_ERROR),
            ("409 Conflict", CONFLICT_ERROR),
            ("413 Too Large", INVALID_ERROR),
            ("422 Unprocessable", INVALID_ERROR),
            ("429 Too Many Requests", RATE_LIMIT_ERROR),
            ("302 Found", UNAVAILABLE_ERROR),
        ] {
            let (url, task) = server(status, "{}".into());
            assert_eq!(
                submit_with_policy(
                    &url,
                    &review(),
                    Duration::from_secs(2),
                    &[Duration::ZERO; 2],
                )
                .unwrap_err(),
                message
            );
            assert_eq!(task.join().unwrap().len(), 1);
        }
    }

    #[test]
    fn transient_retry_attempts_are_bounded() {
        let (url, task) = server_sequence(
            (0..3)
                .map(|_| {
                    (
                        Some(("503 Unavailable".into(), "{}".into())),
                        Duration::ZERO,
                    )
                })
                .collect(),
        );
        assert_eq!(
            submit_with_policy(
                &url,
                &review(),
                Duration::from_secs(2),
                &[Duration::ZERO; 2],
            )
            .unwrap_err(),
            UNAVAILABLE_ERROR
        );
        assert_eq!(task.join().unwrap().len(), 3);
    }

    #[test]
    fn timeout_stays_within_the_shared_submission_budget() {
        let (url, task) = server_sequence(vec![
            (
                Some(("503 Unavailable".into(), "{}".into())),
                Duration::ZERO,
            ),
            (
                Some(("200 OK".into(), receipt_body())),
                Duration::from_secs(2),
            ),
        ]);
        let started = Instant::now();
        assert!(submit_with_policy(
            &url,
            &review(),
            Duration::from_secs(1),
            &[Duration::ZERO; 2],
        )
        .is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(task.join().unwrap().len(), 2);
    }
}
