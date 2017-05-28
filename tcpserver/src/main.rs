extern crate netcode;

use netcode::AESClient;

use std::thread;
use std::clone::Clone;
use std::net::TcpListener;

fn do_caps(stream: &mut AESClient, args: &[&str]) -> Result<bool, String> {
	if args.len() < 2 { return Err(String::from("no args provided!")) }

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

	stream.write_string_enc(&result);

	Ok(true)
}

fn do_ping(stream: &mut AESClient, args: &[&str]) -> Result<bool, String> {
	if args.len() > 1 { return Err(String::from("too many arguments!")) }

	stream.write_string_enc("pong");

	Ok(true)
}

fn do_help(stream: &mut AESClient, args: &[&str]) -> Result<bool, String> {
	match args.len() {
		1 => {
			stream.write_string_enc("--- command list ---\n");

			for command in &COMMANDS {
				stream.write_string_enc(command.usage);
				stream.write_string_enc("");
			}

			stream.write_string_enc("--- end of command list ---");
		},

		2 => {
			match get_command_by_name(args[1]) {
				Some(command) => stream.write_string_enc(command.usage),
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
	function: fn(&mut AESClient, &[&str]) -> Result<bool, String>,
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

const COMMANDS: [Command;3] = [ // TODO: auto-fill length?
	Command {
		name: "caps",
		usage: "caps [string]: echo a string back after converting it to all caps",
		function: do_caps,
	},

	Command {
		name: "ping",
		usage: "ping: server will return the string 'pong'",
		function: do_ping,
	},

	Command {
		name: "help",
		usage: "help <command>: print a the usage string for a command, or all commands if one is not specified",
		function: do_help,
	},
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
fn execute_command(command: Command, stream: &mut AESClient, args: &[&str]) -> bool {
	let cont: bool;

	match (command.function)(stream, args) {
		Ok(value) => {
			cont = value; // pass along the cont value provided by the executor function
		},

		Err(why) => {
			stream.write_string_enc(&why);

			if args.len() > 0 {
				stream.write_string_enc("");
				stream.write_string_enc(&format!("usage: {}", command.short_usage().unwrap()));
			}

			cont = true; // note: can never kill connection because of a reported command error
		},
	}

	stream.write_string_enc("endresponse");
	cont
}


fn send_shutdown_notification(stream: &mut AESClient) {
	stream.write_string_enc("endconn");
}

fn main() {
	let listener = TcpListener::bind("127.0.0.1:8650").unwrap();
	println!("listening started, ready to accept\n");

	for stream in listener.incoming() {
		thread::spawn(|| {
			let mut stream = stream.unwrap();
			let mut stream = AESClient::from_server_socket(&mut stream);

			println!("[server] negotiated key = {:?}\n", stream.key);
			// TODO: short opcode function to ensure successful communication

			stream.write_string_enc("simple application-layer server");
			stream.write_string_enc("for a list of commands, send 'help'");
			stream.write_string_enc("endheader");

			loop {
				let input: String = stream.read_string_enc();
				// println!("in:'{}'", input);

				let args: Vec<&str> = input.split(" ").collect();

				if args.len() == 0 {
					stream.write_string_enc("error: recieved empty command");
					stream.write_string_enc("for a list of commands, send 'help'");
					stream.write_string_enc("endresponse");
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
								stream.write_string_enc("error: unknown command");
								stream.write_string_enc("for a list of commands, send 'help'");
								stream.write_string_enc("endresponse");
							}
						}
					},
				}
			}
		});
	}
}
