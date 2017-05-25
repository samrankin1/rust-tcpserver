extern crate byteorder;

use std::net::TcpStream;

use std::io::Read;
use std::io::Write;
use std::io::Cursor;

use byteorder::NetworkEndian;
use byteorder::ByteOrder;
use byteorder::ReadBytesExt;

fn net_encode_u64(data: u64) -> Vec<u8> {
	let mut bytes: [u8; 8] = [0; 8]; // 64 bits = 8 bytes
	NetworkEndian::write_u64(&mut bytes, data);

	let mut result: Vec<u8> = Vec::with_capacity(8);
	for byte in &bytes {
		result.push(*byte);
	}

	result
}

fn net_decode_u64(encoded: &[u8]) -> u64 { // always a u64 for maximum compatibility
	Cursor::new(encoded).read_u64::<NetworkEndian>().unwrap()
}

fn net_encode_string(data: &str) -> Vec<u8> {
	data.as_bytes().to_vec()
}

fn net_decode_string(encoded: &[u8]) -> String {
	String::from_utf8(encoded.to_vec())
		.expect("utf8 error decoding string")
}

pub trait Netcode {

	fn write_bytes(&mut self, bytes: &[u8]);
	fn write_bytes_auto(&mut self, bytes: &[u8]);

	fn read_bytes(&mut self, count: u64) -> Vec<u8>;
	fn read_bytes_auto(&mut self, max_count: u64) -> Vec<u8>;

	fn write_string(&mut self, data: &str);
	fn read_string(&mut self) -> String;

	fn send_shutdown_notification(&mut self);

}

impl Netcode for TcpStream {

	fn write_bytes(&mut self, bytes: &[u8]) {
		self.write_all(bytes)
			.expect("failed to write bytes to stream");
	}

	fn write_bytes_auto(&mut self, bytes: &[u8]) {
		let encoded_len: Vec<u8> = net_encode_u64(bytes.len() as u64);

		// println!("len = {}", bytes.len());
		// println!("length bytes: u64 = {:?}", encoded_len);
		// println!("data: [u8; {}] = {:?}", bytes.len(), bytes);

		self.write_bytes(&encoded_len);
		self.write_bytes(bytes);
	}

	fn read_bytes(&mut self, count: u64) -> Vec<u8> {
		// println!("len = {}", count);

		let mut result: Vec<u8> = vec![0; count as usize]; // TODO: "count as usize" may be unsafe
		self.read_exact(&mut result)
			.expect("failed to read bytes");

		// println!("bytes: [u8; {}] = {:?}", count, result);

		result
	}

	fn read_bytes_auto(&mut self, max_count: u64) -> Vec<u8> {
		let len_bytes: Vec<u8> = self.read_bytes(8);

		// println!("length bytes: u64 = {:?}", len_bytes);

		let len: u64 = net_decode_u64(&len_bytes);

		// hard limit on memory allocated per read call (and packet size)
		// prevents malicious or malformed packets from demanding large buffers
		if len > max_count {
			return Vec::new(); // return empty set of bytes, signaling failure
		}

		self.read_bytes(len)
	}

	fn write_string(&mut self, data: &str) {
		let encoded: Vec<u8> = net_encode_string(data);
		self.write_bytes_auto(&encoded);
	}

	fn read_string(&mut self) -> String {
		let bytes = self.read_bytes_auto(1024); // read max of 1024 bytes
		net_decode_string(&bytes)
	}

	fn send_shutdown_notification(&mut self) {
		self.write_string("endconn");
	}

}
