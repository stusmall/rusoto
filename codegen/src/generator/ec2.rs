use inflector::Inflector;

use botocore::{Member, Operation, Service, Shape, ShapeType};
use generator::capitalize_first;
use std::borrow::Cow;
use super::GenerateProtocol;
use super::generate_field_name;
use super::tests::{Response, find_responses};

pub struct Ec2Generator;

impl GenerateProtocol for Ec2Generator {
    fn generate_methods(&self, service: &Service) -> String {
        service.operations.values().map(|operation| {
            format!(
                "{documentation}
                {method_signature} {{
                    let mut request = SignedRequest::new(\"{http_method}\", \"{endpoint_prefix}\", self.region, \"{request_uri}\");
                    let mut params = Params::new();

                    params.put(\"Action\", \"{operation_name}\");
                    params.put(\"Version\", \"{api_version}\");

                    {serialize_input}

                    request.set_params(params);
                    request.sign(&try!(self.credentials_provider.credentials()));

                    let result = try!(self.dispatcher.dispatch(&request));

                    match result.status {{
                        200 => {{
                            let mut reader = EventReader::from_str(&result.body);
                            let mut stack = XmlResponse::new(reader.events().peekable());
                            stack.next();
                            {method_return_value}
                        }},
                        _ => Err({error_type}::from_body(&result.body))
                    }}
                }}
                ",
                documentation = generate_documentation(operation),
                http_method = &operation.http.method,
                endpoint_prefix = &service.metadata.endpoint_prefix,
                method_return_value = generate_method_return_value(operation),
                method_signature = generate_method_signature(operation),
                operation_name = &operation.name,
                error_type = operation.error_type_name(),
                api_version = service.metadata.api_version,
                request_uri = &operation.http.request_uri,
                serialize_input = generate_method_input_serialization(operation),
            )
        }).collect::<Vec<String>>().join("\n")
    }

    fn generate_prelude(&self, _service: &Service) -> String {
        "use std::str::{FromStr, from_utf8};

        use xml::EventReader;

        use param::{Params, ServiceParams};

        use signature::SignedRequest;
        use xml::reader::events::XmlEvent;
        use xmlutil::{Next, Peek, XmlParseError, XmlResponse};
        use xmlutil::{characters, end_element, start_element, skip_tree};
        use xmlerror::*;

        enum DeserializerNext {
            Close,
            Skip,
            Element(String),
        }
        ".to_owned()
    }

    fn generate_struct_attributes(&self, _struct_name: &str) -> String {
        "#[derive(Debug, Default, Clone)]".to_owned()
    }

    fn generate_support_types(&self, name: &str, shape: &Shape, _service: &Service) -> Option<String> {
        let mut struct_collector = String::new();
        let serializer = generate_serializer_body(name, shape);

        if serializer.is_some() {
            struct_collector.push_str(&format!("
            /// Serialize `{name}` contents to a `SignedRequest`.
            struct {name}Serializer;
            impl {name}Serializer {{
                {serializer_signature} {{
                    {serializer_body}
                }}
            }}",
            name = name,
            serializer_signature = generate_serializer_signature(name, shape),
            serializer_body = serializer.unwrap())
            );
        }
        let deserializer = generate_deserializer_body(name, shape);
        if deserializer.is_some() {
            struct_collector.push_str(&format!(
            "/// Deserializes `{name}` from XML.
            struct {name}Deserializer;
            impl {name}Deserializer {{
                #[allow(unused_variables)]
                fn deserialize<'a, T: Peek + Next>(tag_name: &str, stack: &mut T)
                -> Result<{name}, XmlParseError> {{
                    {deserializer_body}
                }}
            }}",
            name = name,
            deserializer_body = deserializer.unwrap())
            );
        }
        Some(struct_collector)
    }

    fn timestamp_type(&self) -> &'static str {
        "String"
    }

    fn generate_tests(&self, service: &Service) -> Option<String> {
        Some(format!(
            "
            #[cfg(test)]
            mod protocol_tests {{
                {tests_body}
            }}
            ",
            tests_body = generate_tests_body(service)
        ))
    }
}

fn generate_response_parse_test(service: &Service, response: Response) -> Option<String> {
    let maybe_operation = service.operations.get(&response.action);

    if maybe_operation.is_none() {
        return None;
    }

    let operation = maybe_operation.unwrap();
    let input_shape = operation.input_shape();

    Some(format!("
    #[test]
    fn test_parse_{service_name}_{action}() {{
        let mock_response =  MockResponseReader::read_response(\"{response_file_name}\");
        let mock = MockRequestDispatcher::with_status(200)
            .with_body(&mock_response);
        let client = {client_type}::with_request_dispatcher(mock, MockCredentialsProvider, rusoto_region::UsEast1);
        let request = {request_type}::default();
        let result = client.{action_method}(&request);
        if result.is_err() {{
            println!(\"result: {{:?}}\", result);
        }}
        assert!(result.is_ok());
    }}
    ",
    service_name=response.service.to_snake_case(),
    action=response.action.to_snake_case(),
    response_file_name=response.file_name,
    client_type=service.client_type_name(),
    request_type=input_shape,
    action_method=operation.name.to_snake_case()))
}

fn generate_tests_body(service: &Service) -> String {
    let responses: Vec<Response> = find_responses();

    let our_responses: Vec<Response> = responses
        .into_iter()
        .filter(|r| r.service == service.service_type_name())
        .collect();

    let test_bodies: Vec<String> = our_responses
        .into_iter()
        .flat_map(|response| generate_response_parse_test(service, response))
        .collect();

    let tests_str = test_bodies
        .join("\n\n");

    format!("
        use mock::*;
        use super::*;
        use super::super::Region as rusoto_region;
        {test_bodies}
    ",
    test_bodies=tests_str)
}

fn generate_documentation(operation: &Operation) -> String {
    match operation.documentation {
        Some(ref docs) => format!("#[doc=\"{}\"]", docs.replace("\"", "\\\"").replace("C:\\", "C:\\\\")),
        None => "".to_owned(),
    }
}

fn generate_method_input_serialization(operation: &Operation) -> String {
    if operation.input.is_some() {
        format!(
            "{input_type}Serializer::serialize(&mut params, \"\", &input);",
            input_type = operation.input.as_ref().unwrap().shape,
        )
    } else {
        String::new()
    }
}

fn generate_response_tag_name<'a>(member_name: &'a str) -> Cow<'a, str> {
    if member_name.ends_with("Result") {
        format!("{}Response", &member_name[..member_name.len()-6]).into()
    } else {
        member_name.into()
    }
}

fn generate_method_return_value(operation: &Operation) -> String {
    if operation.output.is_some() {
        let output_type = &operation.output.as_ref().unwrap().shape;
        let standard_tag_name = generate_response_tag_name(output_type).into_owned();
        let tag_name = match standard_tag_name.as_ref() {
            // These all have "Create $service_definition_output_name Response" format:
            "Snapshot" => "CreateSnapshotResponse",
            "Volume" => "CreateVolumeResponse",
            "KeyPair" => "CreateKeyPairResponse",
            "InstanceAttribute" => "DescribeInstanceAttributeResponse",
            "Reservation" => "RunInstancesResponse",
            // Except these because we can't have nice things:
            "VolumeAttachment" => {
                match operation.name.as_ref() {
                    "DetachVolume" => "DetachVolumeResponse",
                    "AttachVolume" => "AttachVolumeResponse",
                    _ => &standard_tag_name,
                }
            },
            _ => &standard_tag_name
        };

        format!(
            "Ok(try!({output_type}Deserializer::deserialize(\"{tag_name}\", &mut stack)))",
            output_type = output_type,
            tag_name = tag_name
        )
    } else {
        "Ok(())".to_owned()
    }
}

fn generate_method_signature(operation: &Operation) -> String {
    if operation.input.is_some() {
        format!(
            "pub fn {operation_name}(&self, input: &{input_type}) -> Result<{output_type}, {error_type}>",
            input_type = operation.input.as_ref().unwrap().shape,
            operation_name = operation.name.to_snake_case(),
            output_type = &operation.output_shape_or("()"),
            error_type = operation.error_type_name(),
        )
    } else {
        format!(
            "pub fn {operation_name}(&self) -> Result<{output_type}, {error_type}>",
            operation_name = operation.name.to_snake_case(),
            error_type = operation.error_type_name(),
            output_type = &operation.output_shape_or("()"),
        )
    }
}

fn generate_deserializer_body(name: &str, shape: &Shape) -> Option<String> {
    // Requests don't get deserialized, except the ones that do.
    if name.ends_with("Request") {
        match name {
            "CancelledSpotInstanceRequest" => (),
            "PurchaseRequest" => (),
            "SpotInstanceRequest" => (),
            _ => return None,
        }
    }
    match shape.shape_type {
        ShapeType::List => Some(generate_list_deserializer(shape)),
        ShapeType::Structure => Some(generate_struct_deserializer(name, shape)),
        _ => Some(generate_primitive_deserializer(shape)),
    }
}

fn generate_list_deserializer(shape: &Shape) -> String {

    let location_name = shape.member.as_ref().and_then(|m| m.location_name.as_ref()).map(|name| &name[..]).unwrap_or(shape.member());

    format!(
        "
        let mut obj = vec![];
        try!(start_element(tag_name, stack));

        loop {{
            let next_event = match stack.peek() {{
                Some(&XmlEvent::EndElement {{ .. }}) => DeserializerNext::Close,
                Some(&XmlEvent::StartElement {{ ref name, .. }}) => DeserializerNext::Element(name.local_name.to_owned()),
                _ => DeserializerNext::Skip,
            }};

            match next_event {{
                DeserializerNext::Element(name) => {{
                    if name == \"{location_name}\" {{
                        obj.push(try!({member_name}Deserializer::deserialize(\"{location_name}\", stack)));
                    }} else {{
                        skip_tree(stack);
                    }}
                }},
                DeserializerNext::Close => {{
                    try!(end_element(tag_name, stack));
                    break;
                }}
                DeserializerNext::Skip => {{ stack.next(); }},
            }}
        }}

        Ok(obj)
        ",
        location_name = location_name,
        member_name = shape.member()
    )
}

fn generate_primitive_deserializer(shape: &Shape) -> String {
    let statement =  match shape.shape_type {
        ShapeType::String | ShapeType::Timestamp => "try!(characters(stack))",
        ShapeType::Integer => "i32::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Long => "i64::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Double => "f64::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Float => "f32::from_str(try!(characters(stack)).as_ref()).unwrap()",
        ShapeType::Blob => "try!(characters(stack)).into_bytes()",
        ShapeType::Boolean => "bool::from_str(try!(characters(stack)).as_ref()).unwrap()",
        shape_type => panic!("Unknown primitive shape type: {:?}", shape_type),
    };

    format!(
        "try!(start_element(tag_name, stack));
        let obj = {statement};
        try!(end_element(tag_name, stack));

        Ok(obj)
        ",
        statement = statement,
    )
}

fn generate_struct_deserializer(name: &str, shape: &Shape) -> String {
    if shape.members.as_ref().unwrap().is_empty() {
        return format!(
            "try!(start_element(tag_name, stack));
            stack.next();

            let obj = {name}::default();

            try!(end_element(tag_name, stack));
            stack.next();

            Ok(obj)
            ",
            name = name,
        );
    }

    format!(
        "try!(start_element(tag_name, stack));

        let mut obj = {name}::default();

        loop {{
            let next_event = match stack.peek() {{
                Some(&XmlEvent::EndElement {{ .. }}) => DeserializerNext::Close,   // TODO verify that we received the expected tag?
                Some(&XmlEvent::StartElement {{ ref name, .. }}) => DeserializerNext::Element(name.local_name.to_owned()),
                _ => DeserializerNext::Skip,
            }};

            match next_event {{
                DeserializerNext::Element(name) => {{
                    match &name[..] {{
                        {struct_field_deserializers}
                        _ => skip_tree(stack),
                    }}
                }},
                DeserializerNext::Close => break,
                DeserializerNext::Skip => {{ stack.next(); }},
            }}
        }}

        try!(end_element(tag_name, stack));

        Ok(obj)
        ",
        name = name,
        struct_field_deserializers = generate_struct_field_deserializers(shape),
    )
}

fn generate_struct_field_deserializers(shape: &Shape) -> String {
    shape.members.as_ref().unwrap().iter().map(|(member_name, member)| {
        // look up member.shape in all_shapes.  use that shape.member.location_name
        let location_name = member.location_name.as_ref().unwrap_or(member_name);

        let parse_expression = generate_struct_field_parse_expression(shape, member_name, member, member.location_name.as_ref());
        format!(
            "\"{location_name}\" => {{
                obj.{field_name} = {parse_expression};
            }}",
            field_name = generate_field_name(member_name),
            parse_expression = parse_expression,
            location_name = location_name,
        )

    }).collect::<Vec<String>>().join("\n")
}

fn generate_struct_field_parse_expression(
    shape: &Shape,
    member_name: &str,
    member: &Member,
    location_name: Option<&String>,
) -> String {

    let location_to_use = match location_name {
        Some(loc) => loc.to_string(),
        None => member_name.to_string(),
    };
    let expression = format!(
        "try!({name}Deserializer::deserialize(\"{location}\", stack))",
        name = member.shape,
        location = location_to_use,
    );

    if shape.required(member_name) {
        expression
    } else {
        format!("Some({})", expression)
    }
}

fn generate_serializer_body(name: &str, shape: &Shape) -> Option<String> {
    // Don't need to send "Response" objects, don't make the code for their serializers
    if name.ends_with("Response") {
        return None;
    }
    match shape.shape_type {
        ShapeType::List => Some(generate_list_serializer(shape)),
        ShapeType::Map => Some(generate_map_serializer(shape)),
        ShapeType::Structure => Some(generate_struct_serializer(shape)),
        _ => Some(generate_primitive_serializer(shape)),
    }
}

fn generate_serializer_signature(name: &str, shape: &Shape) -> String {
    if shape.shape_type == ShapeType::Structure && shape.members.as_ref().unwrap().is_empty() {
        format!("fn serialize(_params: &mut Params, name: &str, _obj: &{})", name)
    } else {
        format!("fn serialize(params: &mut Params, name: &str, obj: &{})", name)
    }
}

fn generate_list_serializer(shape: &Shape) -> String {
    format!(
        "for (index, element) in obj.iter().enumerate() {{
    let key = format!(\"{{}}.{{}}\", name, index);
    {name}Serializer::serialize(params, &key, element);
}}
        ",
        name = shape.member(),
    )
}

fn generate_map_serializer(shape: &Shape) -> String {
    format!(
        "for (index, (key, value)) in obj.iter().enumerate() {{
    let prefix = format!(\"{{}}.{{}}\", name, index);
    {key_name}Serializer::serialize(
        params,
        &format!(\"{{}}.{{}}\", prefix, \"{key_name}\"),
        key,
    );
    {value_name}Serializer::serialize(
        params,
        &format!(\"{{}}.{{}}\", prefix, \"{value_name}\"),
        value,
    );
}}
        ",
        key_name = shape.key(),
        value_name = shape.value(),
    )
}

fn generate_struct_serializer(shape: &Shape) -> String {
    format!(
        "let mut prefix = name.to_string();
if prefix != \"\" {{
    prefix.push_str(\".\");
}}

{struct_field_serializers}
        ",
        struct_field_serializers = generate_struct_field_serializers(shape),
    )
}

fn generate_struct_field_serializers(shape: &Shape) -> String {
    shape.members.as_ref().unwrap().iter().map(|(member_name, member)| {
        let tag_name = capitalize_first(member.location_name.as_ref().unwrap_or(member_name).as_ref());
        if shape.required(member_name) {
            format!(
                "{member_shape_name}Serializer::serialize(
    params,
    &format!(\"{{}}{tag_name}\", prefix),
    &obj.{field_name},
);
                ",
                field_name = generate_field_name(member_name),
                member_shape_name = member.shape,
                tag_name = tag_name,
            )
        } else {
            format!(
                "if let Some(ref field_value) = obj.{field_name} {{
    {member_shape_name}Serializer::serialize(
        params,
        &format!(\"{{}}{tag_name}\", prefix),
        field_value,
    );
}}
                ",
                field_name = generate_field_name(member_name),
                member_shape_name = member.shape,
                tag_name = tag_name,
            )
        }
    }).collect::<Vec<String>>().join("\n")
}

fn generate_primitive_serializer(shape: &Shape) -> String {
    let expression = match shape.shape_type {
        ShapeType::String | ShapeType::Timestamp => "obj",
        ShapeType::Integer | ShapeType::Long | ShapeType::Float | ShapeType::Double | ShapeType::Boolean => "&obj.to_string()",
        ShapeType::Blob => "from_utf8(obj).unwrap()",
        shape_type => panic!("Unknown primitive shape type: {:?}", shape_type),
    };

    format!("params.put(name, {});", expression)
}
