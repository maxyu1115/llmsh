use log;
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

pub struct BufferParser<S: Copy + PartialEq + Eq + Hash, E: Copy> {
    // buffer storing all the buffered input
    input_buffer: Vec<u8>,
    // how much of the buffer is parsed already
    parsed_length: usize,
    // current state
    state: S,

    // state -> (transition_condition, next_state, emitted_event)
    state_map: HashMap<S, Vec<(TransitionCondition, S, E)>>,
}

impl<S: Copy + PartialEq + Eq + Hash, E: Copy> BufferParser<S, E> {
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
        for (condition, state, event) in &self.state_map[&self.state] {
            match condition {
                TransitionCondition::StringID(identifier, visible) => {
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
                        log::debug!("Parsed length {}, end {}", self.parsed_length, end);
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
