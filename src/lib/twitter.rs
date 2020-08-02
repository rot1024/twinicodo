use chrono::{DateTime, TimeZone, Utc};
use derive_more::From;
use futures::stream::{try_unfold, TryStream};
use itertools::Itertools;
use lazy_static::lazy_static;
use reqwest::{
    header::{HeaderMap, HeaderValue, InvalidHeaderValue},
    Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{borrow::Cow, collections::HashMap, fmt, num::ParseIntError, str::FromStr};
use thiserror::Error;

const DEFAULT_PARAMS: &[(&str, &str)] = &[
    ("include_profile_interstitial_type", "1"),
    ("include_blocking", "1"),
    ("include_blocked_by", "1"),
    ("include_followed_by", "1"),
    ("include_want_retweets", "1"),
    ("include_mute_edge", "1"),
    ("include_can_dm", "1"),
    ("include_can_media_tag", "1"),
    ("skip_status", "1"),
    ("cards_platform", "Web-12"),
    ("include_cards", "1"),
    ("include_ext_alt_text", "true"),
    ("include_quote_count", "true"),
    ("include_reply_count", "1"),
    ("tweet_mode", "extended"),
    ("include_entities", "true"),
    ("include_user_entities", "true"),
    ("include_ext_media_color", "true"),
    ("include_ext_media_availability", "true"),
    ("send_error_codes", "true"),
    ("simple_quoted_tweet", "true"),
    ("tweet_search_mode", "live"),
    ("count", "100"), // defaults 20
    ("query_source", "typed_query"),
    ("pc", "1"),
    ("spelling_corrections", "1"),
    ("ext", "mediaStats%2ChighlightedLabel"),
];

#[derive(Debug, Deserialize, Serialize)]
pub struct Cookie {
    pub auth_token: String,
    pub twitter_sess: String,
    pub ct0: String,
}

#[derive(Debug, Error, From)]
pub enum CookieError {
    #[error("cookie auth_token missing")]
    AuthTokenMissing,
    #[error("cookie _twitter_sess missing")]
    TwitterSessMissing,
    #[error("cookie ct0 missing")]
    Ct0Missing,
}

fn cookie_map<'a>(s: &'a str) -> HashMap<&'a str, &'a str> {
    s.split(";")
        .map(|s| s.trim().trim_end_matches(';').split("=").next_tuple())
        .filter_map(|c| c)
        .collect()
}

impl FromStr for Cookie {
    type Err = CookieError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let map = cookie_map(s);

        let auth_token = map.get("auth_token").ok_or(CookieError::AuthTokenMissing)?;
        let twitter_sess = map
            .get("_twitter_sess")
            .ok_or(CookieError::TwitterSessMissing)?;
        let ct0 = map.get("ct0").ok_or(CookieError::Ct0Missing)?;

        Ok(Self {
            auth_token: auth_token.to_string(),
            twitter_sess: twitter_sess.to_string(),
            ct0: ct0.to_string(),
        })
    }
}

impl fmt::Display for Cookie {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "auth_token={}; _twitter_sess={}; ct0={}",
            self.auth_token, self.twitter_sess, self.ct0
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Auth {
    pub authorization_token: String,
    pub csrf_token: String,
    pub cookie: Cookie,
}

impl Auth {
    fn headers(&self) -> Result<HeaderMap<HeaderValue>, InvalidHeaderValue> {
        let mut h = HeaderMap::new();
        h.append(
            "authorization",
            format!("Bearer {}", self.authorization_token).parse()?,
        );
        h.append("x-csrf-token", self.csrf_token.parse()?);
        h.append("cookie", format!("{}", self.cookie).parse()?);
        Ok(h)
    }
}

#[derive(Debug, Error, From)]
pub enum Error {
    #[error("{0}")]
    Request(reqwest::Error),
    #[error("{0}")]
    URL(url::ParseError),
}

#[derive(Debug)]
pub struct TwitterClient {
    client: reqwest::Client,
}

impl TwitterClient {
    pub fn new<A: Into<Auth>>(auth: A) -> Result<Self, ()> {
        Ok(Self {
            client: reqwest::Client::builder()
                .default_headers(auth.into().headers().map_err(|_| ())?)
                .build()
                .unwrap(),
        })
    }

    pub fn search_tweets(&self, query: Query) -> impl TryStream<Ok = Response, Error = Error> {
        lazy_static! {
            static ref URL: Url = Url::parse_with_params(
                "https://api.twitter.com/2/search/adaptive.json",
                DEFAULT_PARAMS,
            )
            .unwrap();
        }

        struct Context {
            client: reqwest::Client,
            query: String,
            cursor: Option<String>,
            finished: bool,
        }

        let ctx = Context {
            client: self.client.clone(),
            query: query.to_string(),
            cursor: None,
            finished: false,
        };

        try_unfold(ctx, |ctx| async move {
            if ctx.finished {
                return Ok(None);
            }

            let mut req = ctx.client.get(URL.to_owned()).query(&[("q", &ctx.query)]);
            if let Some(cursor) = ctx.cursor.as_ref() {
                req = req.query(&[("cursor", cursor)]);
            }

            let res = req
                .send()
                .await?
                .error_for_status()?
                .json::<RawResponse>()
                .await?;

            let cursor = res.next_cursor().map(|s| s.to_string());
            if res.global_objects.tweets.is_empty() || cursor.is_none() {
                Ok(Some((
                    res.into(),
                    Context {
                        cursor,
                        finished: true,
                        ..ctx
                    },
                )))
            } else {
                Ok(Some((res.into(), Context { cursor, ..ctx })))
            }
        })
    }
}

#[derive(Debug)]
pub struct Query {
    pub since: Option<String>,
    pub until: Option<String>,
    pub text: String,
}

impl ToString for Query {
    fn to_string(&self) -> String {
        let mut q = vec![Cow::from(&self.text)];
        if let Some(d) = self.since.as_ref() {
            q.push(Cow::from(format!("since:{}", d)))
        }
        if let Some(d) = self.until.as_ref() {
            q.push(Cow::from(format!("until:{}", d)))
        }
        q.join(" ")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tweet {
    pub id: String,
    pub created_at: Option<DateTime<Utc>>,
    pub full_text: String,
    pub user_id: String,
    pub extra: HashMap<String, Value>,
    pub user: Option<User>,
}

impl From<RawTweet> for Tweet {
    fn from(tweet: RawTweet) -> Self {
        let created_at = tweet.id_str.datetime().ok();
        Self {
            id: tweet.id_str.0,
            created_at,
            full_text: tweet.full_text,
            user_id: tweet.user_id_str,
            extra: tweet.extra,
            user: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: u64,
    pub id_str: String,
    pub name: String,
    pub screen_name: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

pub type Response = Vec<Tweet>;

impl From<RawResponse> for Response {
    fn from(res: RawResponse) -> Self {
        let users = res.global_objects.users;

        res.global_objects
            .tweets
            .into_iter()
            .map(|(_, tweet)| {
                let user = users.get(&tweet.user_id_str).map(|u| u.clone());
                (tweet, user)
            })
            .map(|(t, u)| Tweet {
                user: u,
                ..t.into()
            })
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RawResponse {
    #[serde(rename = "globalObjects")]
    global_objects: GlobalObjects,
    timeline: Timeline,
}

impl RawResponse {
    fn next_cursor(&self) -> Option<&str> {
        self.timeline
            .instructions
            .iter()
            .filter_map(|i| {
                i.add_entries
                    .as_ref()
                    .map(|e| e.entries.iter().collect())
                    .or_else(|| i.replace_entry.as_ref().map(|e| vec![&e.entry]))
                    .map(|e| e.into_iter())
            })
            .flatten()
            .find(|e| e.entry_id == "sq-cursor-bottom")
            .and_then(|e| e.content.operation.as_ref())
            .map(|o| &o.cursor.value as &str)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GlobalObjects {
    tweets: HashMap<String, RawTweet>,
    users: HashMap<String, User>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RawTweet {
    pub id_str: TweetID,
    pub full_text: String,
    pub user_id: u64,
    pub user_id_str: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Timeline {
    id: String,
    instructions: Vec<Instruction>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Instruction {
    #[serde(rename = "addEntries")]
    pub add_entries: Option<AddEntries>,
    #[serde(rename = "replaceEntry")]
    replace_entry: Option<ReplaceEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AddEntries {
    entries: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplaceEntry {
    #[serde(rename = "entryIdToReplace")]
    entry_id_to_replace: String,
    entry: Entry,
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    #[serde(rename = "entryId")]
    entry_id: String,
    #[serde(rename = "sortIndex")]
    sort_index: String,
    content: EntryContent,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryContent {
    operation: Option<EntryContentOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryContentOperation {
    cursor: EntryCursor,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryCursor {
    value: String,
    // #[serde(rename = "cursor_type")]
    // cursorType: String,
}

#[test]
fn test_cookie_map() {
    let mut expected = HashMap::<&str, &str>::new();
    expected.insert("foo", "bar");
    expected.insert("hoge", "bar");
    assert_eq!(expected, cookie_map("foo=bar; hoge=bar"));
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TweetID(pub String);

impl TweetID {
    fn datetime(&self) -> Result<DateTime<Utc>, ParseIntError> {
        Ok(Utc.timestamp_millis((self.0.parse::<i64>()? >> 22) + 1288834974657))
    }
}

#[test]
fn test_tweet_id() {
    assert_eq!(
        Utc.timestamp_millis(1596385521282),
        TweetID("1289960487912783872".to_string())
            .datetime()
            .unwrap()
    );
}
