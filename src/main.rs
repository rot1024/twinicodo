use clap::Clap;
use derive_more::From;
use dialoguer::{theme::ColorfulTheme, Input};
use futures::stream::TryStreamExt;
use std::fs::File;
use thiserror::Error;
use tokio::task::{spawn_blocking, JoinError};
use twinicodo::{
    iter::SortedTweetToChat,
    nicodo::{write_xml, Chat, XMLError},
    twitter::{CookieError, Error as TwitterError, Query, TwitterClient},
};

mod config;

type MainResult<T> = Result<T, MainError>;

#[derive(Error, Debug, From)]
enum MainError {
    #[error("{0}")]
    IO(std::io::Error),
    #[error("{0}")]
    XML(XMLError),
    #[error("{0}")]
    Confy(confy::ConfyError),
    #[error("{0}")]
    Twitter(TwitterError),
    #[error("{0}")]
    Join(JoinError),
    #[error("{0}")]
    Cookie(CookieError),
    #[error("auth error")]
    Auth,
}

#[derive(Debug, Clap)]
#[clap(about = "A command line tool to search tweets and convert into niconico XML file")]
struct Opts {
    text: String,
    #[clap(long, short)]
    since: String,
    #[clap(long, short)]
    until: String,
    #[clap(long, short)]
    output: Option<String>,
    #[clap(long)]
    reset: bool,
}

#[tokio::main]
async fn main() -> MainResult<()> {
    let opts = Opts::parse();
    let output = opts
        .output
        .as_ref()
        .map(|o| o.to_string())
        .unwrap_or_else(|| format!("{}_{}_{}.xml", &opts.text, &opts.since, &opts.until));

    let mut settings = config::Config::load()?;
    if opts.reset || !settings.init || !settings.validate() {
        settings = init()?;
    }
    let settings = settings;

    let client = TwitterClient::new(settings).map_err(|_| MainError::Auth)?;
    let query = Query {
        text: opts.text.to_string(),
        since: Some(opts.since),
        until: Some(opts.until),
    };

    // TODO: write json async, and converting into XML will be executed after writing JSON
    let tweets = client
        .search_tweets(query)
        .inspect_ok(|r| {
            if r.len() > 0 {
                eprintln!(
                    "{} tweets: {}/{} {} {}",
                    r.len(),
                    r.first()
                        .and_then(|t| t.user.as_ref())
                        .map(|u| &u.screen_name as &str)
                        .unwrap_or(""),
                    r.first().map(|t| &t.id as &str).unwrap_or(""),
                    r.first()
                        .and_then(|t| t.created_at)
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_default(),
                    r.first()
                        .map(|t| t
                            .full_text
                            .chars()
                            .take(20)
                            .collect::<String>()
                            .replace("\n", ""))
                        .unwrap_or_default()
                );
            }
        })
        .try_collect::<Vec<_>>()
        .await?;

    let tweets = tweets
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_iter()
        .map_to_sorted_chats()
        .collect::<Vec<_>>();

    if tweets.is_empty() {
        eprintln!("No tweet found.");
        return Ok(());
    }

    let tweets = spawn_blocking(move || -> MainResult<Vec<Chat>> {
        let w = File::create(output)?;
        write_xml(w, tweets.iter())?;
        Ok(tweets)
    })
    .await??;

    eprintln!("{} tweets are saved!", tweets.len());
    Ok(())
}

fn init() -> MainResult<config::Config> {
    eprintln!("Please provide Twitter auth information!");

    let theme = ColorfulTheme {
        ..ColorfulTheme::default()
    };

    let authorization_token = Input::with_theme(&theme)
        .with_prompt("Authorization bearer token")
        .interact()?;
    let csrf_token = Input::with_theme(&theme)
        .with_prompt("CSRF token")
        .interact()?;
    let cookie = Input::with_theme(&theme).with_prompt("Cookie").interact()?;

    let cfg = config::Config::from_cookie(authorization_token, csrf_token, cookie)?;
    cfg.store()?;
    Ok(cfg)
}
