extern crate byteorder;

use std::io::Read;
use std::io::Write;
use std::io::Cursor;
use std::clone::Clone;
use std::net::TcpStream;

use std::net::TcpListener;
use std::thread;

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

trait Netcode {

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



fn do_caps(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {
	if args.len() < 2 {
		return Err(String::from("no args provided!"));
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

	stream.write_string(&result);

	Ok(true)
}

fn do_help(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {
	match args.len() {
		1 => {
			stream.write_string("--- command list ---\n");

			for command in &COMMANDS {
				stream.write_string(command.usage);
				stream.write_string("");
			}

			stream.write_string("--- end of command list ---");
		},

		2 => {
			match get_command_by_name(args[1]) {
				Some(command) => stream.write_string(command.usage),
				None => return Err(format!("no command found with name '{}'!", args[1])),
			}
		},

		_ => return Err(String::from("too many arguments!")),
	}

	Ok(true)
}


struct Command<'a> {
	name: &'a str,
	usage: &'a str,
	function: fn(&mut TcpStream, &[&str]) -> Result<bool, String>,
}

impl<'a> Clone for Command<'a> {
	fn clone(&self) -> Self {
		Command {
			name: self.name,
			usage: self.usage,
			function: self.function,
		}
	}
}

impl<'a> Command<'a> {
	fn short_usage(&self) -> Result<&str, &str> {
		match (&self).usage.find(':') {
			Some(last_index) => Ok(&(&self).usage[0..last_index]),
			None => Err("err: no semicolon delimiter found in command's usage string!")
		}
	}
}

const COMMANDS: [Command;2] = [ // TODO: auto-fill length?
	Command {
		name: "caps",
		usage: "caps [string]: echo a string back after converting it to all caps",
		function: do_caps,
	},

	Command {
		name: "help",
		usage: "help <command>: print a the usage string for a command, or all commands if one is not specified",
		function: do_help,
	}
];

fn get_command_by_name(command_str: &str) -> Option<Command> {
	for command in &COMMANDS {
		if command.name == command_str { return Some(command.clone()) }
	}

	None
}

// Execute the provided command function for the given stream and args
// If the Command returns an Err, the Command's usage string will be sent to the client
// Returns wheter or not to continue listening for commands from the sender
fn execute_command(command: Command, stream: &mut TcpStream, args: &[&str]) -> bool {
	let cont: bool;

	match (command.function)(stream, args) {
		Ok(value) => {
			cont = value; // pass along the cont value provided by the executor function
		},

		Err(why) => {
			stream.write_string(&why);

			if args.len() > 0 {
				stream.write_string("");
				stream.write_string(&format!("usage: {}", command.short_usage().unwrap()));
			}

			cont = true; // note: can never kill connection because of a reported command error
		},
	}

	stream.write_string("endresponse");
	cont
}


fn main() {
	let listener = TcpListener::bind("127.0.0.1:8650").unwrap();
	println!("listening started, ready to accept");
	for stream in listener.incoming() {
		thread::spawn(|| {
			let mut stream = stream.unwrap();

			stream.write_string("simple application-layer server");
			stream.write_string("for a list of commands, send 'help'");
			stream.write_string("endheader");

			loop {
				let input: String = stream.read_string();
				// println!("in:'{}'", input);

				let args: Vec<&str> = input.split(" ").collect();

				if args.len() == 0 {
					stream.write_string("error: recieved empty command");
					stream.write_string("for a list of commands, send 'help'");
					stream.write_string("endresponse");
					continue;
				}

				let cmd: &str = args[0];

				println!("cmd = '{}'", cmd);

				match cmd {
					// special cases and packets
					"endconn" => {
						println!("recieved shutdown notification from client");
						break;
					},

					_ => {
						match get_command_by_name(cmd) {
							Some(command) => {
								if !execute_command(command, &mut stream, &args) {
									stream.send_shutdown_notification();
									break;
								}
							},

							None => {
								stream.write_string("error: unknown command");
								stream.write_string("for a list of commands, send 'help'");
								stream.write_string("endresponse");
							}
						}
					},
				}
			}
		});
	}
}
