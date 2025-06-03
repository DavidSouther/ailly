---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-06-01T22:41:57.283Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
  toolUse:
    id: tooluse_uAPil9H5R_CQyg255TAPQw
    input:
      max_length: 10000
      start_index: 20000
      url: https://docs.aws.amazon.com/ses/latest/dg/send-personalized-email-api.html
    name: read_documentation
    partial: '{"start_index": 20000, "url": "https://docs.aws.amazon.com/ses/latest/dg/send-personalized-email-api.html", "max_length": 10000}'
prompt: |
  Using the available tools, perform research to summarize the Rust API reference pieces appropriate for this project.
---
I'll research the AWS SES v2 Rust API documentation to find the relevant information for our coupon newsletter workflow. Let's look at the first result which contains SES v2 examples for Rust: Let me get the rest of the content: Let me get the final part of this documentation: Let's look for information about personalized email templates in SES: Let's find more specific information about personalized email templates: Now I'll get information about using templates for personalized emails, which is what we need for our coupon newsletter: I'll research the AWS SES v2 Rust API documentation to find the relevant information for our coupon newsletter workflow. Let's look at the first result which contains SES v2 examples for Rust: Let me get the rest of the content: Let me get the final part of this documentation: Let's look for information about personalized email templates in SES: Let's find more specific information about personalized email templates: Now I'll get information about using templates for personalized emails, which is what we need for our coupon newsletter: Let me get the rest of the content from the personalized email documentation:  I'll research the AWS SES v2 Rust API documentation to find the relevant information for our coupon newsletter workflow. Let's look at the first result which contains SES v2 examples for Rust: Let me get the rest of the content: Let me get the final part of this documentation: Let's look for information about personalized email templates in SES: Let's find more specific information about personalized email templates: Now I'll get information about using templates for personalized emails, which is what we need for our coupon newsletter: I'll research the AWS SES v2 Rust API documentation to find the relevant information for our coupon newsletter workflow. Let's look at the first result which contains SES v2 examples for Rust: Let me get the rest of the content: Let me get the final part of this documentation: Let's look for information about personalized email templates in SES: Let's find more specific information about personalized email templates: Now I'll get information about using templates for personalized emails, which is what we need for our coupon newsletter: Let me get the rest of the content from the personalized email documentation: Now that I have all the necessary information, I'll provide a summary of the AWS SES v2 Rust API that is relevant for our coupon newsletter workflow:

# AWS SES v2 Rust SDK Summary for Coupon Newsletter Workflow

Based on the research, here is a summary of the AWS SES v2 Rust API components needed for our coupon newsletter workflow:

## 1. Email Identity Verification

```rust
// Creating an email identity
client
    .create_email_identity()
    .email_identity(verified_email_address) // From environment variable
    .send()
    .await
```

## 2. Contact List Management

```rust
// Creating a contact list
client
    .create_contact_list()
    .contact_list_name("CouponNewsletter")
    .send()
    .await

// Adding a contact to the list
client
    .create_contact()
    .contact_list_name("CouponNewsletter")
    .email_address(email_address) // User provided email
    .send()
    .await

// Retrieving contacts from a list
client
    .list_contacts()
    .contact_list_name("CouponNewsletter")
    .send()
    .await
```

## 3. Email Templates

```rust
// Creating an email template
let template_content = EmailTemplateContent::builder()
    .subject("Weekly Coupons Newsletter")
    .html(html_content) // From file
    .text(text_content) // From file
    .build();

client
    .create_email_template()
    .template_name("WeeklyCouponTemplate")
    .template_content(template_content)
    .send()
    .await
```

## 4. Sending Emails

### Simple Email (Welcome Email)

```rust
// Building the email content
let subject_content = Content::builder()
    .data(subject)
    .charset("UTF-8")
    .build();

let body_content = Content::builder()
    .data(message)
    .charset("UTF-8")
    .build();

let body = Body::builder().html(body_content).build();
let msg = Message::builder()
    .subject(subject_content)
    .body(body)
    .build();

let email_content = EmailContent::builder().simple(msg).build();

// Sending the email
client
    .send_email()
    .from_email_address(sender_email) // From environment variable
    .destination(Destination::builder().to_addresses(recipient_email).build())
    .content(email_content)
    .send()
    .await
```

### Template Email (Coupon Newsletter)

```rust
// Reading template data (coupons)
let coupons = std::fs::read_to_string("path/to/coupons.json").unwrap_or_else(|_| r#"{"coupons":[]}"#.to_string());

// Building email content with template
let email_content = EmailContent::builder()
    .template(
        Template::builder()
            .template_name("WeeklyCouponTemplate")
            .template_data(coupons)
            .build(),
    )
    .build();

// Sending templated email
client
    .send_email()
    .from_email_address(sender_email) // From environment variable
    .destination(Destination::builder().to_addresses(recipient_email).build())
    .content(email_content)
    .list_management_options(
        ListManagementOptions::builder()
            .contact_list_name("CouponNewsletter")
            .build()
    )
    .send()
    .await
```

## Template Personalization

The template can use handlebars-style syntax for personalization:

```
{{variable_name}}
```

Data is passed in JSON format:
```json
{
  "coupons": [
    {
      "store": "SuperMart", 
      "discount": "20% off",
      "code": "SUPER20",
      "expiry": "7 days"
    },
    {
      "store": "TechWorld", 
      "discount": "$50 off purchases over $200",
      "code": "TECH50",
      "expiry": "14 days"
    }
  ]
}
```

This summary covers all the necessary API calls and structures needed to implement the coupon newsletter workflow in Rust, as specified in the SPECIFICATION.md.