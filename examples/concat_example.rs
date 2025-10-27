use serde_json::json;
use harmony_jolt::{transform, TransformSpec};

fn main() {
    // Example: Concatenating first and last name
    let input = json!({
        "customer": {
            "firstName": "John",
            "lastName": "Doe",
            "contact": {
                "email": "john@example.com"
            }
        }
    });

    let spec: TransformSpec = serde_json::from_value(json!([
        {
            "operation": "shift",
            "spec": {
                "customer": {
                    "firstName": "name[0]",
                    "lastName": "name[1]",
                    "contact": {
                        "email": "email"
                    }
                }
            }
        },
        {
            "operation": "modify-overwrite-beta",
            "spec": {
                "name": "=concat(@(1,name[0]),' ',@(1,name[1]))"
            }
        }
    ]))
    .expect("Failed to parse spec");

    let output = transform(input, &spec).expect("Transform failed");

    println!("Output:");
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    // Expected output:
    // {
    //   "name": "John Doe",
    //   "email": "john@example.com"
    // }
}
