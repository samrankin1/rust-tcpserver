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

fn net_encode_usize(data: usize) -> Vec<u8> {
	let mut bytes: [u8; 8] = [0; 8]; // 64 bits = 8 bytes
	NetworkEndian::write_u64(&mut bytes, data as u64);

	let mut result: Vec<u8> = Vec::with_capacity(8);
	for i in 0..8 {
		result.push(bytes[i]);
	}

	result
}

fn net_decode_usize(encoded: &[u8]) -> u64 { // always a u64 for maximum compatibility
	Cursor::new(encoded).read_u64::<NetworkEndian>().unwrap()
}

fn net_encode_string(data: &str) -> Vec<u8> {
	data.as_bytes().to_vec()
}

fn net_decode_string(encoded: &[u8]) -> String {
	String::from_utf8(encoded.to_vec())
		.expect("utf8 error decoding string")
}

/* TODO:
impl TcpStream {
	fn write_bytes(&mut self, bytes: &[u8]) -> usize {

	}

	...
}
*/

// TODO: universally migrate usize -> u64
// TODO: write_bytes and read_bytes automatic retry until entire buffer is sent
fn write_bytes(stream: &mut TcpStream, bytes: &[u8]) -> usize {
	stream.write(bytes)
		.expect("failed to write bytes to stream")
}

fn write_bytes_auto(stream: &mut TcpStream, bytes: &[u8]) -> usize {
	let encoded_len: Vec<u8> = net_encode_usize(bytes.len());

	// println!("len = {}", bytes.len());
	// println!("length bytes: u64 = {:?}", encoded_len);
	// println!("data: [u8; {}] = {:?}", bytes.len(), bytes);

	write_bytes(stream, &encoded_len);

	write_bytes(stream, bytes)
}

fn read_bytes(stream: &mut TcpStream, count: u64) -> Vec<u8> {
	// println!("len = {}", count);

	let mut result: Vec<u8> = vec![0; count as usize]; // TODO: "count as usize" may be unsafe
	stream.read(&mut result)
		.expect("failed to read bytes");

	// println!("bytes: [u8; {}] = {:?}", count, result);

	result
}

fn read_bytes_auto(stream: &mut TcpStream, max_count: u64) -> Vec<u8> {
	let len_bytes: Vec<u8> = read_bytes(stream, 8);

	// println!("length bytes: u64 = {:?}", len_bytes);

	let len: u64 = net_decode_usize(&len_bytes);

	// hard limit on memory allocated per read call (and packet size)
	// prevents malicious or malformed packets from demanding large buffers
	if len > max_count { // if the
		return Vec::new(); // return empty set of bytes, signaling failure
	}

	read_bytes(stream, len)
}

/* not used yet
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
*/

fn write_string(stream: &mut TcpStream, data: &str) {
	let encoded: Vec<u8> = net_encode_string(data);
	write_bytes_auto(stream, &encoded);
}

fn read_string(stream: &mut TcpStream) -> String {
	let bytes = read_bytes_auto(stream, 1024); // read max of 1024 bytes
	net_decode_string(&bytes)
}

fn send_shutdown_notification(stream: &mut TcpStream) {
	write_string(stream, "endconn");
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

	write_string(stream, &result);

	Ok(true)
}

fn do_help(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {

	match args.len() {
		1 => {
			write_string(stream, "--- command list ---\n");

			for command in &COMMANDS {
				write_string(stream, command.usage);
				write_string(stream, "");
			}

			write_string(stream, "--- end of command list ---");
		},

		2 => {
			match get_command_by_name(args[1]) {
				Some(command) => write_string(stream, command.usage),
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
			write_string(stream, &why);

			if args.len() > 0 {
				write_string(stream, "");
				write_string(stream, &format!("usage: {}", command.short_usage().unwrap()));
			}

			cont = true; // note: can never kill connection because of a reported command error
		},
	}

	write_string(stream, "endresponse");
	cont
}


fn main() {
	let listener = TcpListener::bind("127.0.0.1:8650").unwrap();
	println!("listening started, ready to accept");
	for stream in listener.incoming() {
		thread::spawn(|| {
			let mut stream = stream.unwrap();

			write_string(&mut stream, "simple application-layer server");
			write_string(&mut stream, "for a list of commands, send 'help'");
			write_string(&mut stream, "endheader");

			loop {
				let input: String = read_string(&mut stream);
				// println!("in:'{}'", input);

				let args: Vec<&str> = input.split(" ").collect();

				if args.len() == 0 {
					write_string(&mut stream, "error: recieved empty command");
					write_string(&mut stream, "for a list of commands, send 'help'");
					write_string(&mut stream, "endresponse");
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
									send_shutdown_notification(&mut stream);
									break;
								}
							},

							None => {
								write_string(&mut stream, "error: unknown command");
								write_string(&mut stream, "for a list of commands, send 'help'");
								write_string(&mut stream, "endresponse");
							}
						}
					},
				}
			}
		});
	}
}
