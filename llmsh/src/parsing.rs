use lazy_static;
use log;
use regex::Regex;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

#[derive(PartialEq, Debug)]
pub enum StepResults<E: Copy> {
    Echo(Vec<u8>),
    StateChange {
        event: E,
        step: Vec<u8>,
        aggregated: Vec<u8>,
    },
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

    pub fn step(&mut self) -> Option<StepResults<E>> {
        if self.input_buffer.len() == self.parsed_length {
            return Option::None;
        }
        log::debug!(
            "Current state {:?}, input buffer: {:?}",
            self.state,
            self.input_buffer
        );
        // TODO: refactor to look at all conditions at once and transition based on earliest match
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
                        let step: Vec<u8> = if self.parsed_length < end {
                            self.input_buffer[self.parsed_length..end].to_vec()
                        } else {
                            // there's a weird case where part of the (invisible) identifier was in the previous step
                            // this causes the previously parsed_length to exceed the end. We output [] in this case
                            vec![]
                        };

                        // transition successful everything prior to the marker gets outputted
                        let aggregated: Vec<u8> = self.input_buffer.drain(..end).collect();
                        // throw away the identifier if it shouldn't be visible
                        if !*visible {
                            self.input_buffer.drain(..identifier.len());
                        }
                        self.parsed_length = 0;
                        self.state = *state;
                        return Some(StepResults::StateChange {
                            event: *event,
                            step,
                            aggregated,
                        });
                    } else {
                        // if not found, try the next transition
                        continue;
                    }
                }
            }
        }
        let prev_len = self.parsed_length;
        self.parsed_length = self.input_buffer.len();
        return Some(StepResults::Echo(
            self.input_buffer[prev_len..self.parsed_length].to_vec(),
        ));
    }

    pub fn parse(&mut self, input: &[u8]) -> Vec<StepResults<E>> {
        let mut ret = Vec::new();
        self.buffer(input);
        loop {
            match self.step() {
                None => break,
                Some(step) => {
                    ret.push(step);
                }
            }
        }
        return ret;
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

    static ref ANSI_ALLOWED: HashSet<String> = HashSet::from_iter(["\x1b[D".to_string(), "\x1b[C".to_string()]);
}

pub fn strip_ansi_escape_sequences(text: &str) -> String {
    // Iterator to find all matches and filter out left and right arrow keys
    let result: String = ANSI_ESCAPE
        .replace_all(text, |caps: &regex::Captures| {
            let cap = caps.get(0).unwrap().as_str();
            if ANSI_ALLOWED.contains(cap) {
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
    use super::TransitionCondition::StringID;
    use super::*;
    use rstest::rstest;
    use std::collections::HashMap;

    fn one_state_no_transition_parser() -> BufferParser<i32, Option<()>> {
        BufferParser::new(
            0,
            HashMap::from([(
                0,
                vec![(
                    StringID("impossible transition".to_string(), false),
                    0,
                    None,
                )],
            )]),
        )
    }

    fn two_state_bidirection_parser(
        zero_to_one_id: &str,
        one_to_zero_id: &str,
        visible: bool,
    ) -> BufferParser<i32, i32> {
        BufferParser::new(
            0,
            HashMap::from([
                (
                    0,
                    vec![(StringID(zero_to_one_id.to_string(), visible), 1, 0)],
                ),
                (
                    1,
                    vec![(StringID(one_to_zero_id.to_string(), visible), 0, 1)],
                ),
            ]),
        )
    }

    fn to_vec_u8(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    #[test]
    fn test_buffer_parser_empty_case1() {
        let mut parser = one_state_no_transition_parser();
        parser.buffer("".as_bytes());
        assert!(parser.step().is_none());
    }

    #[test]
    fn test_buffer_parser_empty_case2() {
        let mut parser = one_state_no_transition_parser();
        parser.buffer("random input".as_bytes());
        parser.step();
        // after previous input is parsed, it should no longer show up as echo
        assert!(parser.step().is_none());
    }

    #[test]
    fn test_buffer_parser_echo_no_match() {
        let mut parser = one_state_no_transition_parser();
        parser.buffer("Random String".as_bytes());
        let result = parser.step().unwrap();
        match result {
            StepResults::Echo(s) => {
                assert_eq!(s, "Random String".as_bytes());
            }
            _ => panic!("Expected StepResults::Echo"),
        }
    }

    #[rstest]
    #[case("We transition using transition1 to state 1", vec![
        StepResults::StateChange { event: 0, step: to_vec_u8("We transition using transition1"), aggregated: to_vec_u8("We transition using transition1") },
        StepResults::Echo(to_vec_u8(" to state 1")),
    ])]
    #[case("transition0 doesn't work since we are on state 0", vec![
        StepResults::Echo(to_vec_u8("transition0 doesn't work since we are on state 0")),
    ])]
    #[case("transition1transition0transition1", vec![
        StepResults::StateChange { event: 0, step: to_vec_u8("transition1"), aggregated: to_vec_u8("transition1") },
        StepResults::StateChange { event: 1, step: to_vec_u8("transition0"), aggregated: to_vec_u8("transition0") },
        StepResults::StateChange { event: 0, step: to_vec_u8("transition1"), aggregated: to_vec_u8("transition1") },
    ])]
    fn test_buffer_parser_transition(#[case] input: &str, #[case] expected: Vec<StepResults<i32>>) {
        let mut parser = two_state_bidirection_parser("transition1", "transition0", true);
        let result = parser.parse(input.as_bytes());
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case("transition0 is visible", vec![
        StepResults::Echo(to_vec_u8("transition0 is visible")),
    ])]
    #[case("transition1transition0transition1", vec![
        StepResults::StateChange { event: 0, step: vec![], aggregated: vec![] },
        StepResults::StateChange { event: 1, step: vec![], aggregated: vec![] },
        StepResults::StateChange { event: 0, step: vec![], aggregated: vec![] },
    ])]
    fn test_buffer_parser_id_invisible(
        #[case] input: &str,
        #[case] expected: Vec<StepResults<i32>>,
    ) {
        let mut parser = two_state_bidirection_parser("transition1", "transition0", false);
        let result = parser.parse(input.as_bytes());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_buffer_parser_identifier_segmented() {
        let mut parser = two_state_bidirection_parser("transition1", "transition0", false);
        let result1 = parser.parse("this input is segmented and tran".as_bytes());
        // This is probably not ideal behavior. TODO: if a string partially matches the identifier, withold it from echoing
        assert_eq!(
            result1,
            vec![StepResults::Echo(to_vec_u8(
                "this input is segmented and tran"
            ))]
        );
        let result2 = parser.parse("sition1 is split in half".as_bytes());
        assert_eq!(
            result2,
            vec![
                StepResults::StateChange {
                    event: 0,
                    step: vec![],
                    aggregated: to_vec_u8("this input is segmented and ")
                },
                StepResults::Echo(to_vec_u8(" is split in half"))
            ]
        );
    }

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
