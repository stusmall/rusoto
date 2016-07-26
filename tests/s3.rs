#![cfg(feature = "s3")]
extern crate env_logger;
extern crate rusoto;

#[macro_use] 
extern crate log;

use rusoto::s3::S3Client;
use rusoto::{DefaultCredentialsProvider, Region};

#[test]
fn should_list_buckets() {
	let _ = env_logger::init();
    let credentials = DefaultCredentialsProvider::new().unwrap();
    let client = S3Client::new(credentials, Region::UsEast1);

	client.list_buckets().unwrap();
}

