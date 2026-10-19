use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};

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

fn submit_to(endpoint: &str, review: &Review) -> Result<Receipt, String> {
    validate(review)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Feedback could not be sent. Your draft is saved; try again.")?;
    let response =
        client.post(endpoint).json(review).send().map_err(|_| {
            "Feedback could not be sent. Your draft is saved; try again when online."
        })?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("Please wait a few minutes before trying again. Your draft is saved.".into());
    }
    if !response.status().is_success() {
        return Err("Feedback was not accepted. Your draft is saved; try again.".into());
    }
    let mut body = Vec::new();
    response.take(4097).read_to_end(&mut body).map_err(|_| {
        "The feedback receipt could not be read. Your draft is saved; retry safely."
    })?;
    let receipt: Receipt = serde_json::from_slice(&body).map_err(|_| {
        "The feedback receipt could not be verified. Your draft is saved; retry safely."
    })?;
    if body.len() > 4096 || !receipt.accepted || receipt.submission_id != review.submission_id {
        return Err(
            "The feedback receipt could not be verified. Your draft is saved; retry safely.".into(),
        );
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

    fn server(status: &str, body: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/feedback", listener.local_addr().unwrap());
        let status = status.to_string();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
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
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
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
            assert_eq!(submit_to(&url, &review()).is_ok(), accepted);
            task.join().unwrap();
        }
    }
}
