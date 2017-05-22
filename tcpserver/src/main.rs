extern crate byteorder;

use std::io::Read;
use std::io::Write;
use std::io::Cursor;
use std::net::TcpStream;

use std::net::TcpListener;
use std::thread;
use std::ptr;

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

fn write_bytes_auto(stream: &mut TcpStream, bytes: &[u8]) -> usize {
	let encoded_len: Vec<u8> = net_encode_usize(bytes.len());

	// println!("len = {}", bytes.len());
	// println!("length bytes: u64 = {:?}", encoded_len);
	// println!("data: [u8; {}] = {:?}", bytes.len(), bytes);

	write_bytes(stream, &encoded_len);

	write_bytes(stream, bytes)
}

fn write_bytes(stream: &mut TcpStream, bytes: &[u8]) -> usize {
	stream.write(bytes)
		.expect("failed to write bytes to stream")
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

fn write_string(stream: &mut TcpStream, data: &str) { // TODO stream.write_string(&mut self) {...
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

fn _do_unknown_command(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {
	write_string(stream, "unknown command");
	write_string(stream, "for a list of commands, send 'help'");

	Ok(true)
}

fn _do_empty_command(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {
	write_string(stream, "recieved empty command");
	write_string(stream, "for a list of commands, send 'help'");

	Ok(true)
}

// Execute the provided command function for the given stream and args
// Returns wheter or not to continue listening for commands from the sender
fn execute_command(do_command: fn(&mut TcpStream, &[&str]) -> Result<bool, String>, stream: &mut TcpStream, args: &[&str]) -> bool {
	let mut cont: bool = true;

	match do_command(stream, args) {
		Ok(value) => {
			cont = value; // pass along the cont value provided by the executor function
		},
		Err(why) => {
			write_string(stream, &why);

			if args.len() > 0 {
				write_string(stream, "");
				write_string(stream, &format!("for a complete usage string, send 'help {}'", args[0]));
			}

			cont = true; // note: can never kill connection because of a reported command error
		},
	}

	write_string(stream, "endresponse");
	cont
}


const HELP_COMMANDS: Vec<fn(&mut TcpStream, &[&str]) -> Result<bool, String>> = vec![
	do_caps,
	do_help,
];

fn do_help(stream: &mut TcpStream, args: &[&str]) -> Result<bool, String> {

	match args.len() {
		1 => {
			write_string(stream, "--- command list ---");
			for help_string in get_helps() { write_string(stream, &help_string) }
			write_string(stream, "--- end of command list ---");
		},

		2 => {
			match get_command_by_name(args[1]) {
				Some(command_funct) => write_string(stream, &get_help_by_command(command_funct)),
				None => return Err(format!("no command found with name '{}'!", args[1])),
			}
		},

		_ => return Err(String::from("too many arguments!")),
	}

	Ok(true)
}

// command's "name"
fn get_command_by_name(command_str: &str) -> Option<fn(&mut TcpStream, &[&str]) -> Result<bool, String>> {
	match command_str {
		"caps" => Some(do_caps),
		"help" => Some(do_help),
		_ => None,
	}
}

fn get_helps() -> Vec<String> {
	let mut result: Vec<String> = Vec::with_capacity(HELP_COMMANDS.len());
	for command_funct in HELP_COMMANDS {
		result.push(get_help_by_command(command_funct));
	}

	result
}

fn get_help_by_command(command_funct: fn(&mut TcpStream, &[&str]) -> Result<bool, String>) -> String {
	match command_funct {
		// TODO: compare fn's
		a if ptr::eq(a, do_help) => String::from("caps [string]: echo a string back after converting it to all caps"),
		a if ptr::eq(&a, &do_help) => String::from("help <command>: print a the usage string for a command, or all commands if one is not specified"),
		_ => String::from("no help found for this command! please report this bug"), // will not happen
	}
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
					execute_command(_do_empty_command, &mut stream, &args);
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
						let command_funct = get_command_by_name(cmd).unwrap_or(_do_unknown_command); // unwrap command or default to unknown
						if !execute_command(command_funct, &mut stream, &args) {
							send_shutdown_notification(&mut stream);
							break;
						}
					},
				}
			}
		});
	}
}
