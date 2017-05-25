extern crate netcode;
extern crate time;

use netcode::Netcode;

use std::io;
use std::io::Write;
use std::net::TcpStream;

use time::Duration;

fn handle_ping(stream: &mut TcpStream) -> Result<i64, &str> {
	stream.write_string("ping");

	let mut pong: String = String::new();
	let millis: i64 = Duration::span(|| { // TODO: investigate unreliable results
		pong = stream.read_string();
	}).num_milliseconds();

	match pong.as_ref() {
		"pong" => {
			stream.read_string(); // discard "endresponse" message
			Ok(millis)
		}
		_ => Err("unexpected ping response"),
	}
}

// Send the command to the given stream and print response until an "endresponse" message is found
// Returns whether the server will be expecting more input
fn send_command_print_response(stream: &mut TcpStream, command: &str) -> bool {
	stream.write_string(command);

	loop {
		let input: String = stream.read_string();

		match input.as_ref() {
			"endconn" => return false,
			"endresponse" => {
				println!();
				break;
			},

			_ => println!("{}", input),
		}
	}

	true
}

fn main() {
	let mut stream = TcpStream::connect("127.0.0.1:8650")
		.expect("[client] failed to connect to server");

	loop {
		let input: String = stream.read_string();

		match input.as_ref() {
			"endheader" => break,

			_ => println!("{}", input),
		}
	}

	loop {
		print!("server> ");
		io::stdout().flush().unwrap();

		let mut command: String = String::new();
		match io::stdin().read_line(&mut command) {
			Ok(_) => {
				let command: &str = &command.trim();
				match command {
					"exit" => {
						stream.send_shutdown_notification();
						break;
					},

					"ping" => {
						println!("[client] ping timed at {} ms\n", handle_ping(&mut stream).unwrap());
					}

					_ => if !send_command_print_response(&mut stream, command) {
						println!("[client] server indicates it will not be expecting any more commands");
						break;
					},
				}
			},
			Err(error) => println!("[client] error reading from io::stdin(): {}", error),
		}
	}
}
