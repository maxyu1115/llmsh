use lazy_static;
use log;
use regex::Regex;
use std::collections::HashMap;
use std::hash::Hash;

pub enum StepResults<'a, E> {
    Echo(&'a [u8]),
    StateChange {
        event: E,
        step: Vec<u8>,
        aggregated: Vec<u8>,
    },
    Done,
}

pub enum TransitionCondition {
    // identifier, visible
    StringID(String, bool),
}

pub struct BufferParser<S: Copy + PartialEq + Eq + Hash + std::fmt::Debug, E: Copy> {
    // buffer storing all the buffered input
    input_buffer: Vec<u8>,
    // how much of the buffer is parsed already
    parsed_length: usize,
    // current state
    state: S,

    // state -> (transition_condition, next_state, emitted_event)
    state_map: HashMap<S, Vec<(TransitionCondition, S, E)>>,
}

impl<S: Copy + PartialEq + Eq + Hash + std::fmt::Debug, E: Copy> BufferParser<S, E> {
    pub fn new(
        state: S,
        state_map: HashMap<S, Vec<(TransitionCondition, S, E)>>,
    ) -> BufferParser<S, E> {
        return BufferParser {
            input_buffer: Vec::with_capacity(4096),
            parsed_length: 0,
            state,
            state_map,
        };
    }

    pub fn buffer(&mut self, input: &[u8]) {
        self.input_buffer.extend_from_slice(input);
    }

    pub fn step(&mut self) -> StepResults<E> {
        if self.input_buffer.is_empty() {
            return StepResults::Done;
        }
        log::debug!(
            "Current state {:?}, input buffer: {:?}",
            self.state,
            self.input_buffer
        );
        for (condition, state, event) in &self.state_map[&self.state] {
            match condition {
                TransitionCondition::StringID(identifier, visible) => {
                    log::debug!("Matching on identifier [{}]", identifier);
                    // start at parsed_length - identifier_length to deal with wrap around cases
                    let start = if self.parsed_length < identifier.len() {
                        0
                    } else {
                        self.parsed_length - identifier.len()
                    };
                    let sub_slice = &self.input_buffer[start..];
                    // search for the identifier
                    if let Some(i) = sub_slice
                        .windows(identifier.len())
                        .position(|window| window == identifier.as_bytes())
                    {
                        let end = if *visible {
                            start + i + identifier.len()
                        } else {
                            start + i
                        };
                        log::debug!(
                            "Parsed length {}, identifier.len {}, start {}, i {}, end {}",
                            self.parsed_length,
                            identifier.len(),
                            start,
                            i,
                            end
                        );
                        let step: Vec<u8> = self.input_buffer[self.parsed_length..end].to_vec();

                        // transition successful everything prior to the marker gets outputted
                        let aggregated: Vec<u8> = self.input_buffer.drain(..end).collect();
                        // throw away the identifier if it shouldn't be visible
                        if !*visible {
                            self.input_buffer.drain(..identifier.len());
                        }
                        self.parsed_length = 0;
                        self.state = *state;
                        return StepResults::StateChange {
                            event: *event,
                            step,
                            aggregated,
                        };
                    } else {
                        // if not found, try the next transition
                        continue;
                    }
                }
            }
        }
        let prev_len = self.parsed_length;
        self.parsed_length = self.input_buffer.len();
        return StepResults::Echo(&self.input_buffer[prev_len..self.parsed_length]);
    }
}

lazy_static::lazy_static! {
    // Regular expression to match ANSI escape sequences
    // NOTE THAT the order is very important, escape codes should try to match OSC before C1 for example
    static ref ANSI_ESCAPE: Regex = Regex::new(r"(?x)
        (\x1B\][^\x07]*\x07) |                 # OSC sequences
        (\x1B[\[\?][0-9;]*[a-zA-Z]) |          # CSI sequences
        (\x1B[FG]) |                           # FE sequences
        (\x1B\[\d*(;\d*)*m) |                  # SGR sequences
        (\x1B[@-_][0-?]*[ -/]*[@-~]) |         # C1 control codes
        \x07                                   # bell character
    ").unwrap();
}

pub fn strip_ansi_escape_sequences(text: &str) -> String {
    // Iterator to find all matches and filter out left and right arrow keys
    let result: String = ANSI_ESCAPE
        .replace_all(text, |caps: &regex::Captures| {
            let cap = caps.get(0).unwrap().as_str();
            if cap == "\x1b[D" || cap == "\x1b[C" {
                cap.to_string()
            } else {
                String::new()
            }
        })
        .to_string();

    return result;
}

#[cfg(test)]
mod tests {
    // Import the parent module's items
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        "\x1B[DLeft Arrow\x1B[CRight Arrow",
        "\x1B[DLeft Arrow\x1B[CRight Arrow"
    )]
    #[case("\x1B[31mThis is red text\x1B[0m", "This is red text")]
    #[case("and \x1B]0;Title\x07a title bar text.", "and a title bar text.")]
    fn test_strip_ansi_escape_sequences(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(strip_ansi_escape_sequences(input), expected);
    }
}
