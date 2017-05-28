extern crate netcode;
extern crate time;

use netcode::AESClient;

use std::io;
use std::io::Write;
use std::net::TcpStream;

use time::Duration;

fn handle_ping(stream: &mut AESClient) -> Result<i64, &'static str> {
	stream.write_string_enc("ping");

	let mut pong: String = String::new();
	let millis: i64 = Duration::span(|| { // TODO: investigate unreliable results
		pong = stream.read_string_enc();
	}).num_milliseconds();

	match pong.as_ref() {
		"pong" => {
			stream.read_string_enc(); // discard "endresponse" message
			Ok(millis)
		}
		_ => Err("unexpected ping response"),
	}
}

// Send the command to the given stream and print response until an "endresponse" message is found
// Returns whether the server will be expecting more input
fn send_command_print_response(stream: &mut AESClient, command: &str) -> bool {
	stream.write_string_enc(command);

	loop {
		let input: String = stream.read_string_enc();

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


fn send_shutdown_notification(stream: &mut AESClient) {
	stream.write_string_enc("endconn");
}

fn main() {
	let mut stream = TcpStream::connect("127.0.0.1:8650")
		.expect("[client] failed to connect to server");

	let mut stream = AESClient::from_client_socket(&mut stream);

	println!("[client] negotiated key = {:?}\n", stream.key);
	// TODO: short opcode function to ensure successful communication

	loop {
		let input: String = stream.read_string_enc();

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
						send_shutdown_notification(&mut stream);
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
