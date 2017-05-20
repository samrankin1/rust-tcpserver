extern crate byteorder;

use std::io::Read;
use std::io::Write;
use std::io::Cursor;
use std::net::TcpStream;

use std::io;

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

	write_bytes(stream, &encoded_len);
	let bytes_sent = write_bytes(stream, bytes);

	bytes_sent
}

fn write_bytes(stream: &mut TcpStream, bytes: &Vec<u8>) -> usize {
	stream.write(bytes)
		.expect("failed to write bytes to stream")
}

fn read_bytes_auto(stream: &mut TcpStream, max_count: u64) -> Vec<u8> {
	let len_bytes: Vec<u8> = read_bytes(stream, 8);

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



fn send_command_print_response(stream: &mut TcpStream, command: String) -> bool {
	write_string(stream, command);

	loop {
		let input: String = read_string(stream);

		match input.as_ref() {
			"endconn" => return false,
			"endresponse" => break,
			_ => println!("[RESPONSE] '{}'", input),
		}
	}

	true
}

fn main() {
	let mut stream = TcpStream::connect("127.0.0.1:8650")
		.expect("failed to connect to server");

	loop {
		let input: String = read_string(&mut stream);

		match input.as_ref() {
			"endheader" => break,
			_ => println!("[HEADER] '{}'", input),
		}
	}

	loop {
		print!("server> ");
		io::stdout().flush().unwrap();

		let mut command: String = String::new();
		match io::stdin().read_line(&mut command) {
			Ok(_) => {
				command.pop(); // remove trailing newline TODO: better trim method
				if !send_command_print_response(&mut stream, command) { break }
			},
			Err(error) => println!("error reading from io::stdin(): {}", error),
		}
	}

	println!("{}", "recieved shutdown signal from remote host");
}
