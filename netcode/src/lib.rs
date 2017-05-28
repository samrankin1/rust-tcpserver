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

pub trait Netcode {

	fn write_bytes(&mut self, bytes: &[u8]);
	fn write_bytes_auto(&mut self, bytes: &[u8]);

	fn read_bytes(&mut self, count: u64) -> Vec<u8>;
	fn read_bytes_auto(&mut self, max_count: u64) -> Vec<u8>;

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

}

extern crate ring;
extern crate untrusted;

use ring::{agreement, rand, pbkdf2, aead};
use ring::rand::SecureRandom;
use ring::agreement::EphemeralPrivateKey;

static ECDH_CURVE: &ring::agreement::Algorithm = &agreement::X25519; // Curve25519, as described in RFC 7748
const PUB_KEY_LEN: usize = 32; // X25519 takes 32 bytes as a key

static PBKDF2_PRF: &ring::pbkdf2::PRF = &pbkdf2::HMAC_SHA256; // use SHA-256 as the pseudorandom fuction for PBKDF2
const PBKDF2_ITERATIONS: u32 = 10000; // TODO: time alternatives or make this value configurable

static AES_MODE: &ring::aead::Algorithm = &aead::AES_256_GCM; // use 256-bit AES keys in GCM mode
const NONCE_LEN: usize = 12; // 96-bit nonces

struct KeyPair {
	private_key: EphemeralPrivateKey,
	public_encoded: Vec<u8>,
}

impl KeyPair {

	fn generate() -> KeyPair {
		let rng = rand::SystemRandom::new();

		let private_key: EphemeralPrivateKey = agreement::EphemeralPrivateKey::generate(ECDH_CURVE, &rng)
			.expect("failed to generate key pair!");

		let mut public_encoded: Vec<u8> = vec![0; private_key.public_key_len()];
		private_key.compute_public_key(public_encoded.as_mut_slice()) // write the endoded public key to the slice
			.expect("failed to compute public part from private key!");

		KeyPair {
			private_key: private_key,
			public_encoded: public_encoded,
		}
	}

}

struct AESPacket {
	nonce: [u8; NONCE_LEN],
	encrypted_bytes: Vec<u8>,
}

impl AESPacket {

	fn from(nonce: [u8; NONCE_LEN], encrypted_bytes: Vec<u8>) -> AESPacket {
		AESPacket {
			nonce: nonce,
			encrypted_bytes: encrypted_bytes,
		}
	}

	fn from_plaintext(key: &[u8; 32], plaintext: &[u8]) -> AESPacket {
		let key = aead::SealingKey::new(AES_MODE, key)
			.expect("failed to use key bytes as SealingKey!");

		let mut nonce: [u8; NONCE_LEN] = [0; NONCE_LEN];
		{
			let rng = rand::SystemRandom::new();
			rng.fill(&mut nonce)
				.expect("failed to read from secure PRNG");
		}

		// TODO: revise this to eliminate copies (if possible)
		let mut encrypted_bytes: Vec<u8> = plaintext.to_vec();
		encrypted_bytes.reserve_exact(AES_MODE.tag_len());
		for _ in 0..AES_MODE.tag_len() {
			encrypted_bytes.push(0);
		}

		let output_len = aead::seal_in_place(&key, &nonce, &nonce, encrypted_bytes.as_mut_slice(), AES_MODE.tag_len())
			.expect("failed to encrypt bytes with AES!");

		encrypted_bytes.truncate(output_len);

		AESPacket {
			nonce: nonce,
			encrypted_bytes: encrypted_bytes,
		}
	}

	fn to_plaintext(self, key: &[u8; 32]) -> Vec<u8> {
		let key = aead::OpeningKey::new(AES_MODE, key)
			.expect("failed to use key bytes as OpeningKey!");

		// TODO: revise this to eliminate copy (if possible)
		let mut encrypted_bytes: Vec<u8> = self.encrypted_bytes;

		// decrypt the bytes in place
		let decrypted_output: &[u8] = aead::open_in_place(&key, &self.nonce, &self.nonce, 0, &mut encrypted_bytes)
			.expect("failed to decrypt bytes with AES!");

		decrypted_output.to_vec()
	}

}

pub struct AESClient<'a> {
	pub stream: &'a mut TcpStream,
	pub key: [u8; 32], // 32 byte (256 bit) AES keys
}

fn get_handshake_result(my_priv: EphemeralPrivateKey, their_pub: &[u8], server_pub: &[u8], client_pub: &[u8]) -> [u8; 32] {
	agreement::agree_ephemeral(my_priv, ECDH_CURVE, untrusted::Input::from(their_pub), ring::error::Unspecified,
		|key_material: &[u8]| {
			// println!("server_pub ({}) = {:?}\n", server_pub.len(), server_pub);
			// println!("client_pub ({}) = {:?}\n", client_pub.len(), client_pub);
			// println!("key_material ({}) = {:?}\n", key_material.len(), key_material);

			let mut derived_key: [u8; 32] = [0; 32];

			// combine the public keys and shared secret material as a combined secret to feed to the KDF
			{
				let combined_secret: &[u8] = &[server_pub, client_pub, key_material].concat();
				pbkdf2::derive(PBKDF2_PRF, PBKDF2_ITERATIONS, &[0; 8], combined_secret, &mut derived_key); // TODO: fix empty salt?
			}

			Ok(derived_key)
		}
	).expect("handshake failed!")
}

fn net_encode_string(data: &str) -> Vec<u8> {
	data.as_bytes().to_vec()
}

fn net_decode_string(encoded: &[u8]) -> String {
	String::from_utf8(encoded.to_vec())
		.expect("utf8 error decoding string")
}

impl<'a> AESClient<'a> {

	pub fn from_server_socket(server_socket: &mut TcpStream) -> AESClient {
		// recieve public key from the client
		let client_pub: &[u8] = &server_socket.read_bytes(PUB_KEY_LEN as u64); // TODO: verify unsanitized input public key is valid
		// println!("recieved {} encoded public key bytes from the client", client_pub.len());

		let keypair = KeyPair::generate();

		// send our public key to the client
		assert!(keypair.public_encoded.len() == PUB_KEY_LEN);
		server_socket.write_bytes(&keypair.public_encoded);

		AESClient {
			stream: server_socket,
			key: get_handshake_result(keypair.private_key, client_pub, &keypair.public_encoded, client_pub),
		}
	}

	pub fn from_client_socket(client_socket: &mut TcpStream) -> AESClient {
		let keypair = KeyPair::generate();

		// send our public key to the server
		assert!(keypair.public_encoded.len() == PUB_KEY_LEN);
		client_socket.write_bytes(&keypair.public_encoded);

		// recieve public key from the server
		let server_pub: &[u8] = &client_socket.read_bytes(PUB_KEY_LEN as u64);

		AESClient {
			stream: client_socket,
			key: get_handshake_result(keypair.private_key, server_pub, server_pub, &keypair.public_encoded),
		}
	}

	pub fn raw_stream(&mut self) -> &mut TcpStream {
		self.stream
	}

	pub fn write_bytes_enc(&mut self, bytes: &[u8]) {
		let packet = AESPacket::from_plaintext(&self.key, bytes);

		// println!("nonce ({}) = {:?}", packet.nonce.len(), packet.nonce);
		// println!("plain len -> crypto len = {} -> {}", bytes.len(), packet.encrypted_bytes.len());

		self.raw_stream().write_bytes(&packet.nonce);
		self.raw_stream().write_bytes_auto(&packet.encrypted_bytes); // write the encrypted bytes to the stream (with a length)
	}

	pub fn read_bytes_enc(&mut self, max_count: u64) -> Vec<u8> {
		let nonce = self.raw_stream().read_bytes(NONCE_LEN as u64); // the value of NONCE_LEN must be the same between server and client
		let encrypted_bytes = self.raw_stream().read_bytes_auto(max_count); // TODO: reconsider 1024 hard limit on read bytes

		// println!("nonce ({}) = {:?}", nonce.len(), nonce);
		// println!("recieved {} encrypted bytes\n", encrypted_bytes.len());

		let mut nonce_fixed: [u8; NONCE_LEN] = [0; NONCE_LEN];
		nonce_fixed.copy_from_slice(&nonce); // this is safe because read_bytes returns exactly NONCE_LEN bytes

		let packet = AESPacket::from(nonce_fixed, encrypted_bytes);
		packet.to_plaintext(&self.key)
	}

	pub fn write_string_enc(&mut self, data: &str) {
		let encoded = net_encode_string(data);
		self.write_bytes_enc(&encoded);
	}

	pub fn read_string_enc(&mut self) -> String {
		let encoded = self.read_bytes_enc(1024); // 1024 byte limit on incoming encrypted messages
		net_decode_string(&encoded)
	}

}
