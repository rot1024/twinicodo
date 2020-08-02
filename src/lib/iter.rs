use super::{nicodo::Chat, twitter::Tweet};
use lazy_static::lazy_static;
use regex::Regex;
use std::iter::FromIterator;
use std::vec::IntoIter as VecIntoIter;

pub struct TweetToChatIterator<I>(I, u64)
where
    I: ExactSizeIterator<Item = Tweet>;

impl<I> Iterator for TweetToChatIterator<I>
where
    I: ExactSizeIterator<Item = Tweet>,
{
    type Item = Chat;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|t| {
            let date = t.created_at.map(|d| d.timestamp() as u64).unwrap_or(0);
            let vpos = if self.1 == 0 { 0 } else { date - self.1 };
            if self.1 == 0 {
                self.1 = date;
            }

            Self::Item {
                vpos,
                date,
                id: Some(t.id),
                user_id: t.user.map(|u| u.screen_name),
                mail: None,
                content: cleanup(&t.full_text),
            }
        })
    }
}

impl<I> ExactSizeIterator for TweetToChatIterator<I>
where
    I: ExactSizeIterator<Item = Tweet>,
{
    fn len(&self) -> usize {
        self.0.len()
    }
}

pub trait TweetToChat<I: ExactSizeIterator<Item = Tweet>>: ExactSizeIterator<Item = Tweet> {
    fn map_to_chat(self) -> TweetToChatIterator<I>;
}

impl<I> TweetToChat<I> for I
where
    I: ExactSizeIterator<Item = Tweet>,
{
    fn map_to_chat(self) -> TweetToChatIterator<Self> {
        TweetToChatIterator(self, 0)
    }
}

fn cleanup(s: &str) -> String {
    lazy_static! {
        static ref RE_HASHTAG: Regex = Regex::new(r"#[\w_]+[ \t]*").unwrap();
        static ref RE_URL: Regex = Regex::new(r"(?:https?|ftp)://[\n\S]+").unwrap();
    }

    RE_URL
        .replace_all(&RE_HASHTAG.replace_all(s, "").trim(), "")
        .trim()
        .to_string()
}

pub trait SortedTweetToChat<I: ExactSizeIterator<Item = Tweet>>:
    ExactSizeIterator<Item = Tweet>
{
    fn map_to_sorted_chats(self) -> TweetToChatIterator<VecIntoIter<Tweet>>;
}

impl<I> SortedTweetToChat<I> for I
where
    I: ExactSizeIterator<Item = Tweet>,
{
    fn map_to_sorted_chats(self) -> TweetToChatIterator<VecIntoIter<Tweet>> {
        let mut v = Vec::from_iter(self);
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        v.into_iter().map_to_chat()
    }
}
