use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use pure_wav::{Parser, ProcessDataOutput, ReadInstruction};

fn main() {
    let mut file =
        File::open("Warriyo, Laura Brehm - Mortals (feat. Laura Brehm) [NCS Release].wav").unwrap();
    let meta_data = {
        let mut parser = Parser::default();
        let mut buffer = [Default::default(); Parser::MAX_BUFFER_LEN];
        loop {
            let ReadInstruction { position, len } = parser.read_instruction();
            file.seek(SeekFrom::Start(position.into())).unwrap();
            let buffer = &mut buffer[..len.try_into().unwrap()];
            file.read_exact(buffer).unwrap();
            match parser.process_data(&buffer).unwrap() {
                ProcessDataOutput::InProgress(next_parser) => {
                    parser = next_parser;
                }
                ProcessDataOutput::Done(meta_data) => {
                    break meta_data;
                }
            }
        }
    };
    println!("{meta_data:#?}");
}
