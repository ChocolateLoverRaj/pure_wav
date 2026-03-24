# `pure_wav`
[![Crates.io Version](https://img.shields.io/crates/v/pure_wav)](https://crates.io/crates/pure_wav)
[![docs.rs](https://img.shields.io/docsrs/pure_wav)](https://docs.rs/pure_wav/latest/pure_wav/)

A Rust library to parse `.wav` files.

## Features
- `no_std` without `alloc`
- No `unsafe` code
- Very minimal
- Great for playing wav files with I2S on microcontrollers

## Usage
I designed the API to not have async or any kind of callbacks. It can be used no matter how you access the disk, but the API is currently very un-ergonomic. See https://github.com/ChocolateLoverRaj/rust-esp32c3-examples/blob/ca5ab80f178cc1bf08281818cf2877f046f00d45/sd_card_speaker/src/main.rs for example usage.

To get the information necessary to use I2S, create a `GetMetaDataForI2s` with the `Default` trait. Then call the `output` method which will give you a `ReadRequest` telling you what part of the file needs to be read. Read that part of the file and then input it to the `input` method. Eventually you will get a `MetaDataForI2s` which has metadata as well as the range in the file which contains the actual data. Then you can stream the raw data from that range in the file.

## Use cases
- Streaming a file to play a wav file from an SD card
