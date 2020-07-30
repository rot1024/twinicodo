use serde::{Deserialize, Serialize};
use twinicodo::twitter::{Auth, Cookie, CookieError};

const APP_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub authorization_token: String,
    pub csrf_token: String,
    pub cookie_auth_token: String,
    pub cookie_twitter_sess: String,
    pub cookie_ct0: String,
    pub init: bool,
}

impl Config {
    pub fn from_cookie(
        authorization_token: String,
        csrf_token: String,
        cookie: String,
    ) -> Result<Self, CookieError> {
        let cookie = cookie.parse::<Cookie>()?;

        Ok(Self {
            authorization_token,
            csrf_token,
            cookie_auth_token: cookie.auth_token,
            cookie_twitter_sess: cookie.twitter_sess,
            cookie_ct0: cookie.ct0,
            init: true,
        })
    }

    pub fn load() -> Result<Self, confy::ConfyError> {
        confy::load(APP_NAME)
    }

    pub fn store(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, self)
    }

    pub fn validate(&self) -> bool {
        !self.authorization_token.is_empty()
            && !self.csrf_token.is_empty()
            && !self.cookie_auth_token.is_empty()
            && !self.cookie_twitter_sess.is_empty()
            && !self.cookie_ct0.is_empty()
    }
}

impl Into<Auth> for Config {
    fn into(self) -> Auth {
        Auth {
            authorization_token: self.authorization_token,
            csrf_token: self.csrf_token,
            cookie: Cookie {
                auth_token: self.cookie_auth_token,
                twitter_sess: self.cookie_twitter_sess,
                ct0: self.cookie_ct0,
            },
        }
    }
}
