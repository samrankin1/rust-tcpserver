extern crate byteorder;

use std::io::Write;
use std::io::Read;
use std::io::Cursor;
use std::str::Split;
use std::net::TcpListener;
use std::net::TcpStream;

use std::thread;

use byteorder::NetworkEndian;
use byteorder::ByteOrder;
use byteorder::ReadBytesExt;

fn net_encode_usize(data: usize) -> Vec<u8> {
	let mut bytes: [u8; 8] = [0; 8]; // 64 bits = 8 bytes
	NetworkEndian::write_u64(&mut bytes, data as u64);

	let mut result: Vec<u8> = Vec::with_capacity(8);
	for i in 0..8 {
		result.push(bytes[i]);
	}

	result
}

fn net_decode_usize(encoded: Vec<u8>) -> u64 { // actually a u64 (TODO)
	Cursor::new(encoded).read_u64::<NetworkEndian>().unwrap()
}

fn net_encode_string(data: String) -> Vec<u8> {
	data.into_bytes()
}

fn net_decode_string(encoded: Vec<u8>) -> String {
	String::from_utf8(encoded) // TODO: from_utf8_unchecked()? sounds like an exploiter's dream
		.expect("utf8 error decoding string")
}

fn write_bytes_auto(stream: &mut TcpStream, bytes: &Vec<u8>) -> usize {
	let encoded_len: Vec<u8> = net_encode_usize(bytes.len());

	// println!("len = {}", bytes.len());
	// println!("length bytes: u64 = {:?}", encoded_len);
	// println!("data: [u8; {}] = {:?}", bytes.len(), bytes);

	let len_sent: usize = write_bytes(stream, &encoded_len);
	let bytes_sent = write_bytes(stream, bytes);

	// println!("wrote {} + {} bytes", len_sent, bytes_sent);

	bytes_sent
}

fn write_bytes(stream: &mut TcpStream, bytes: &Vec<u8>) -> usize {
	stream.write(bytes)
		.expect("failed to write bytes to stream")
}

fn read_bytes_auto(stream: &mut TcpStream, max_count: u64) -> Vec<u8> {
	let mut len_bytes: Vec<u8> = read_bytes(stream, 8);

	// println!("length bytes: u64 = {:?}", len_bytes);

	let len: u64 = net_decode_usize(len_bytes);

	// hard limit on memory allocated per read call (and packet size)
	// prevents malicious or malformed packets from demanding large buffers
	if len > max_count { // if the
		return Vec::new(); // return empty set of bytes, signaling failure
	}

	read_bytes(stream, len)
}

fn read_bytes(stream: &mut TcpStream, count: u64) -> Vec<u8> {
	// println!("len = {}", count);

	let mut result: Vec<u8> = vec![0; count as usize]; // TODO: "count as usize" may be unsafe
	stream.read(&mut result)
		.expect("failed to read bytes");

	// println!("bytes: [u8; {}] = {:?}", count, result);

	result
}

fn write_i32(stream: &mut TcpStream, data: i32) {
	let mut bytes: [u8; 4] = [0; 4]; // 32 bits = 4 bytes
	NetworkEndian::write_i32(&mut bytes, data);

	let mut byte_vec: Vec<u8> = Vec::with_capacity(4);
	for i in 0..4 {
		byte_vec.push(bytes[i]);
	}

	// assert byte_vec.len() == 4

	write_bytes(stream, &byte_vec);
}

fn read_i32(stream: &mut TcpStream) -> i32 {
	let encoded: Vec<u8> = read_bytes(stream, 4);
	Cursor::new(encoded).read_i32::<NetworkEndian>().unwrap()
}

fn write_string(stream: &mut TcpStream, data: String) { // TODO stream.write_string(&mut self) {...
	let encoded: Vec<u8> = net_encode_string(data);
	write_bytes_auto(stream, &encoded);
}

fn read_string(stream: &mut TcpStream) -> String {
	let bytes = read_bytes_auto(stream, 1024); // read max of 1024 bytes
	net_decode_string(bytes)
}

fn do_help(stream: &mut TcpStream, args: &Vec<&str>) -> bool {
	write_string(stream, String::from("--- command list ---"));
	write_string(stream, String::from("caps [string]: echo a string back after converting it to all caps"));
	write_string(stream, String::from("help: print this list of supported commands"));

	write_string(stream, String::from("endresponse"));

	true
}

fn do_caps(stream: &mut TcpStream, args: &Vec<&str>) -> bool {
	if args.len() < 2 {
		return false;
	}

	let mut result: String = String::new();

	let mut first: bool = true;
	for arg in &args[1..] {
		if first {
			first = false;
		} else {
			result.push_str(" ");
		}

		result.push_str(&arg.to_uppercase());
	}

	write_string(stream, result);

	write_string(stream, String::from("endresponse"));

	true
}

fn do_unknown_command(stream: &mut TcpStream, args: &Vec<&str>) -> bool {
	write_string(stream, String::from("unknown command"));
	write_string(stream, String::from("for a list of commands, send 'help'"));

	write_string(stream, String::from("endresponse"));

	false
}

fn main() {
	let listener = TcpListener::bind("127.0.0.1:8650").unwrap();
	println!("listening started, ready to accept");
	for stream in listener.incoming() {
		thread::spawn(|| {
			let mut stream = stream.unwrap();

			write_string(&mut stream, String::from("simple application-layer server"));
			write_string(&mut stream, String::from("for a list of commands, send 'help'"));

			write_string(&mut stream, String::from("endheader"));

			loop {
				let input: String = read_string(&mut stream);

				let args: Vec<&str> = input.split(" ").collect();

				if args.len() == 0 {
					write_string(&mut stream, String::from("recieved empty command"));
					write_string(&mut stream, String::from("for a list of commands, send 'help'"));
					continue;
				}

				println!("args[0] = '{}'", args[0]);

				match args[0] {
					"caps" => println!("got 'caps' cmd, result = {}", do_caps(&mut stream, &args)),
					"help" => println!("got 'help' cmd, result = {}", do_help(&mut stream, &args)),
					_ => println!("got unknown cmd, result = {}", do_unknown_command(&mut stream, &args)),
				}
			}

			write_string(&mut stream, String::from("endconn"));
		});
	}
}
