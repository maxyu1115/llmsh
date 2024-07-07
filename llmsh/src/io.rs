use std::collections::HashMap;
use std::hash::Hash;
use std::option::Option;


pub struct BufferParser<S: Copy + PartialEq + Eq + Hash> {
    // buffer storing all the buffered input
    input_buffer: Vec<u8>,
    // how much of the buffer is parsed already
    parsed_length: usize,
    // current state
    state: S,

    // state -> (transition_identifier, next_state)
    state_map: HashMap<S, Vec<(String, S)>>,
}


impl <S: Copy + PartialEq + Eq + Hash> BufferParser<S> {
    pub fn new(state: S, state_map: HashMap<S, Vec<(String, S)>>) -> BufferParser<S> {
        return BufferParser {
            input_buffer: Vec::with_capacity(4096),
            parsed_length: 0,
            state: state,
            state_map: state_map,
        };
    }

    pub fn buffer(&mut self, input: &[u8]) {
        self.input_buffer.extend_from_slice(input);
    }

    pub fn step(&mut self) -> Option<(S, Vec<u8>)> {
        if self.input_buffer.is_empty() {
            return None;
        }
        for (identifier, state) in &self.state_map[&self.state] {
            // start at parsed_length - identifier_length to deal with wrap around cases
            let start = if self.parsed_length < identifier.len() { 0 } else { self.parsed_length - identifier.len() };
            let sub_slice = &self.input_buffer[start..];
            // println!("{:?}, {:?}", sub_slice, identifier.as_bytes());
            match sub_slice.windows(identifier.len()).position(|window| window == identifier.as_bytes()) {
                Some(i) => {
                    // transition successful everything prior to the marker gets outputted
                    let output: Vec<u8> = self.input_buffer.drain(..start + i).collect();
                    // throw away the identifier
                    self.input_buffer.drain(..identifier.len());
                    self.parsed_length = 0;
                    self.state = *state;
                    // println!("Matched");
                    return Some((*state, output));
                },
                None => {
                    continue;
                },
            }
        }
        self.parsed_length = self.input_buffer.len();
        return None;
    }
}
