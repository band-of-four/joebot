use crate::{ChainEntry, ChainPrefix, ChainSuffix, Datestamp, MarkovChain, TextSource, NGRAM_CNT};
use chrono::{Datelike, NaiveDateTime};
use indexmap::IndexSet;
use regex::Regex;
use std::collections::HashMap;

use vkopt_message_parser::reader::{fold_html, EventResult, MessageEvent};

pub trait ChainAppend {
    fn append_text(&mut self, input_file: &str, source_name_re: Regex, datestamp: Datestamp);

    fn append_message_dump(
        &mut self,
        input_file: &str,
        short_name_to_re_map: &HashMap<String, Regex>,
    );
}

#[derive(Default)]
struct ExtractedMessage {
    short_name: String,
    datestamp: Datestamp,
    body: String,
}

impl ChainAppend for MarkovChain {
    fn append_text(&mut self, input_file: &str, source_name_re: Regex, datestamp: Datestamp) {
        let text = std::fs::read_to_string(input_file).unwrap();
        let source = source_by_name_re(&mut self.sources, &source_name_re);
        push_text_entries(
            &text,
            datestamp,
            &mut source.entries,
            &mut self.words,
            false,
        );
    }

    fn append_message_dump(
        &mut self,
        input_file: &str,
        short_name_to_re_map: &HashMap<String, Regex>,
    ) {
        let last_msg = fold_html(
            input_file,
            Default::default(),
            |mut msg: ExtractedMessage, event| match event {
                MessageEvent::Start(0) => {
                    if !msg.body.is_empty() {
                        append_message(self, msg, short_name_to_re_map);
                    }
                    EventResult::Consumed(Default::default())
                }
                MessageEvent::ShortNameExtracted(short_name) => {
                    msg.short_name = short_name.to_owned();
                    EventResult::Consumed(msg)
                }
                MessageEvent::DateExtracted(date) => {
                    let timestamp =
                        NaiveDateTime::parse_from_str(date, "%Y.%m.%d %H:%M:%S").unwrap();
                    msg.datestamp = Datestamp {
                        year: timestamp.year() as i16,
                        day: timestamp.ordinal() as u16,
                    };
                    EventResult::Consumed(msg)
                }
                MessageEvent::BodyPartExtracted(body) => {
                    msg.body.push_str(body);
                    EventResult::Consumed(msg)
                }
                _ => EventResult::Consumed(msg),
            },
        )
        .unwrap();
        if !last_msg.body.is_empty() {
            append_message(self, last_msg, short_name_to_re_map);
        }
    }
}

fn source_by_name_re<'a>(sources: &'a mut Vec<TextSource>, name_re: &Regex) -> &'a mut TextSource {
    let idx = sources
        .iter()
        .position(|s| s.name_re.as_str() == name_re.as_str())
        .unwrap_or_else(|| {
            let entries = vec![];
            let new_source = TextSource {
                name_re: name_re.to_owned(),
                entries,
            };
            sources.push(new_source);
            sources.len() - 1
        });
    sources.get_mut(idx).unwrap()
}

fn append_message(
    chain: &mut MarkovChain,
    message: ExtractedMessage,
    short_name_to_re_map: &HashMap<String, Regex>,
) {
    if let Some(name_re) = &short_name_to_re_map.get(&message.short_name) {
        let source = source_by_name_re(&mut chain.sources, name_re);
        push_text_entries(
            &message.body,
            message.datestamp,
            &mut source.entries,
            &mut chain.words,
            true,
        );
    }
}

fn push_text_entries(
    raw_text: &str,
    datestamp: Datestamp,
    entries: &mut Vec<ChainEntry>,
    words: &mut IndexSet<String>,
    treat_newlines_as_terminal: bool,
) {
    let text = raw_text.trim();

    let mut word_indexes: Vec<(u32, bool)> = Vec::new();
    let mut last = 0;
    for (i, matched) in text.match_indices(|c| c == ' ' || c == '\n') {
        if i != last {
            let word = &text[last..i];
            if word.is_empty() {
                continue;
            }
            let terminal = treat_newlines_as_terminal && matched == "\n"
                || word.ends_with(|c| c == '.' || c == '?' || c == '!');
            let word_idx = words.insert_full(word.to_owned()).0 as u32;
            word_indexes.push((word_idx, terminal));
        }
        last = i + matched.len();
    }
    if last < text.len() {
        let word_idx = words.insert_full(text[last..].to_owned()).0 as u32;
        word_indexes.push((word_idx, true));
    }

    if word_indexes.len() < NGRAM_CNT + 1 {
        return;
    }

    let mut is_prefix_starting = true;
    for ngram in word_indexes.windows(NGRAM_CNT + 1) {
        let (prefix_words, suffix) = ngram.split_at(NGRAM_CNT);
        let (suffix_idx, is_suffix_terminal) = suffix[0];
        entries.push(ChainEntry {
            prefix: ChainPrefix::new([prefix_words[0].0, prefix_words[1].0], is_prefix_starting),
            suffix: ChainSuffix::new(suffix_idx, is_suffix_terminal),
            datestamp,
        });
        is_prefix_starting = is_suffix_terminal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::indexset;

    #[test]
    fn test_authors() {
        let mut chain = MarkovChain::new();
        let mut name_map = HashMap::new();
        name_map.insert("sota".into(), Regex::new("so|ta").unwrap());
        name_map.insert("denko".into(), Regex::new("de|n|ko").unwrap());
        chain.append_message_dump("tests/fixtures/messages.html", &name_map);
        assert_eq!(chain.sources[0].name_re.as_str(), "so|ta");
        assert_eq!(chain.sources[1].name_re.as_str(), "de|n|ko");
    }

    #[test]
    fn test_word_nodes() {
        let mut chain = MarkovChain::new();
        let mut name_map = HashMap::new();
        name_map.insert("sota".into(), Regex::new("so|ta").unwrap());
        name_map.insert("denko".into(), Regex::new("de|n|ko").unwrap());
        chain.append_message_dump("tests/fixtures/messages.html", &name_map);
        assert_eq!(chain.words.get_index(0), Some(&"Привет".into()));
        assert_eq!(chain.words.get_index(1), Some(&"Denko".into()));
        assert_eq!(chain.words.get_index(2), Some(&"Пью".into()));

        assert_eq!(
            chain.sources[0].entries[0],
            ChainEntry {
                prefix: ChainPrefix::starting([0, 1]),
                suffix: ChainSuffix::nonterminal(2),
                datestamp: Datestamp {
                    year: 2018,
                    day: 21
                }
            }
        );
        assert_eq!(
            chain.sources[0].entries.last(),
            Some(&ChainEntry {
                prefix: ChainPrefix::starting([3, 4]), // newline
                suffix: ChainSuffix::terminal(5),
                datestamp: Datestamp {
                    year: 2018,
                    day: 21
                }
            })
        );
    }

    #[test]
    fn test_no_empty_words() {
        let mut chain = MarkovChain::new();

        let mut name_map = HashMap::new();
        name_map.insert("sota".into(), Regex::new("so|ta").unwrap());
        name_map.insert("denko".into(), Regex::new("de|n|ko").unwrap());
        chain.append_message_dump("tests/fixtures/messages.html", &name_map);

        let enumerated_words = chain.words.iter().enumerate();
        let empty_words =
            enumerated_words.filter_map(|(i, w)| if w.is_empty() { Some(i) } else { None });
        assert_eq!(empty_words.collect::<Vec<_>>(), vec![0usize; 0]);
    }

    #[test]
    fn test_text() {
        let mut chain = MarkovChain::new();
        chain.append_text(
            "tests/fixtures/text",
            Regex::new("angus|sol onset").unwrap(),
            Datestamp { year: 0, day: 0 },
        );
        assert_eq!(
            chain.words,
            indexset![
                "useless".into(),
                "unreliable".into(),
                "heavily".into(),
                "distorted".into(),
                "probe.".into(),
                "flashing".into(),
                "red.".into()
            ]
        );
        assert_eq!(chain.sources[0].name_re.as_str(), "angus|sol onset");
        assert_eq!(
            chain.sources[0].entries,
            vec![
                ChainEntry {
                    prefix: ChainPrefix::starting([0, 1]),
                    suffix: ChainSuffix::nonterminal(2),
                    datestamp: Datestamp { year: 0, day: 0 }
                },
                ChainEntry {
                    prefix: ChainPrefix::nonstarting([1, 2]),
                    suffix: ChainSuffix::nonterminal(3),
                    datestamp: Datestamp { year: 0, day: 0 }
                },
                ChainEntry {
                    prefix: ChainPrefix::nonstarting([2, 3]),
                    suffix: ChainSuffix::terminal(4),
                    datestamp: Datestamp { year: 0, day: 0 }
                },
                ChainEntry {
                    prefix: ChainPrefix::starting([3, 4]),
                    suffix: ChainSuffix::nonterminal(5),
                    datestamp: Datestamp { year: 0, day: 0 }
                },
                ChainEntry {
                    prefix: ChainPrefix::nonstarting([4, 5]),
                    suffix: ChainSuffix::terminal(6),
                    datestamp: Datestamp { year: 0, day: 0 }
                }
            ]
        );
    }
}
