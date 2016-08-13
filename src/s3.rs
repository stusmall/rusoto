//! AWS S3
 
include!(concat!(env!("OUT_DIR"), "/s3.rs"));

#[cfg(test)]
mod test {
    use s3::{S3Client, HeadObjectRequest};
    use super::super::{Region, DefaultCredentialsProvider, SignedRequest};
    use super::super::mock::*;

    #[test]
    // sample response from the S3 documentation
    // tests the model generation and deserialization end-to-end
    fn should_parse_sample_list_buckets_response() {
        let mock = MockRequestDispatcher::with_status(200)
            .with_body(r#"
            <?xml version="1.0" encoding="UTF-8"?>
            <ListAllMyBucketsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01">
                <Owner>
                <ID>bcaf1ffd86f461ca5fb16fd081034f</ID>
                <DisplayName>webfile</DisplayName>
                </Owner>
                <Buckets>
                <Bucket>
                        <Name>quotes</Name>
                        <CreationDate>2006-02-03T16:45:09.000Z</CreationDate>
                </Bucket>
                <Bucket>
                        <Name>samples</Name>
                        <CreationDate>2006-02-03T16:41:58.000Z</CreationDate>
                </Bucket>
                </Buckets>
            </ListAllMyBucketsResult>
            "#)
            .with_request_checker(|request: &SignedRequest| {
                assert_eq!(request.method, "GET");
                assert_eq!(request.path, "/");
                assert_eq!(request.params.get("Action"),
                           Some(&"ListBuckets".to_string()));
                assert_eq!(request.payload, None);
            });

        let client = S3Client::with_request_dispatcher(mock, MockCredentialsProvider, Region::UsEast1);
        let result = client.list_buckets().unwrap();

        let owner = result.owner.unwrap();
        assert_eq!(owner.display_name, Some("webfile".to_string()));
        assert_eq!(owner.i_d,
                   Some("bcaf1ffd86f461ca5fb16fd081034f".to_string()));

        let buckets = result.buckets.unwrap();
        assert_eq!(buckets.len(), 2);

        let bucket1 = buckets.get(0).unwrap();
        assert_eq!(bucket1.name, Some("quotes".to_string()));
        assert_eq!(bucket1.creation_date,
                   Some("2006-02-03T16:45:09.000Z".to_string()));
    }

   fn should_parse_headers() {
    let credentials = DefaultCredentialsProvider::new().unwrap();
    let mock = MockRequestDispatcher::with_status(200)
        .with_body("")
        .with_header("x-amz-expiration".to_string(), "foo".to_string())
        .with_header("x-amz-restore".to_string(), "bar".to_string());

    let client = S3Client::with_request_dispatcher(mock, credentials, Region::UsEast1);
    let request = HeadObjectRequest::default();
    let result = client.head_object(&request).unwrap();

    assert_eq!(result.expiration, Some("foo".to_string()));
    assert_eq!(result.restore, Some("bar".to_string()));
   }

}
