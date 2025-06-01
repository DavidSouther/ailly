---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-06-01T22:35:59.266Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
  toolUse:
    id: tooluse_n4AfXm6LRUqn34vZnb46oA
    input:
      max_length: 10000
      start_index: 10000
      url: https://docs.aws.amazon.com/code-library/latest/ug/sesv2_example_sesv2_NewsletterWorkflow_section.html
    name: read_documentation
    partial: '{"start_index": 10000, "url": "https://docs.aws.amazon.com/code-library/latest/ug/sesv2_example_sesv2_NewsletterWorkflow_section.html", "max_length": 10000}'
prompt: |
  Using the available tools, perform research to summarize the Python API reference pieces appropriate for this project.
---
I'll research the Python API references relevant for this Amazon SES V2 coupon newsletter workflow project. Let me check the SES V2 Python code examples first: Let me continue reading the documentation: Let me continue reading about the SendEmail API: Let me now research about creating email templates in SES: Let me check the documentation about sending templated emails through Amazon SES: Let me continue reading about the templated email section: Now I'll check for more specific information about working with contact lists in Amazon SES v2: Let me check the documentation for creating contact lists in SES v2: Let's look at a complete Amazon SES v2 newsletter workflow example to understand all the required API calls together: I'll research the Python API references relevant for this Amazon SES V2 coupon newsletter workflow project. Let me check the SES V2 Python code examples first: Let me continue reading the documentation: Let me continue reading about the SendEmail API: Let me now research about creating email templates in SES: Let me check the documentation about sending templated emails through Amazon SES: Let me continue reading about the templated email section: Now I'll check for more specific information about working with contact lists in Amazon SES v2: Let me check the documentation for creating contact lists in SES v2: Let's look at a complete Amazon SES v2 newsletter workflow example to understand all the required API calls together:  I'll research the Python API references relevant for this Amazon SES V2 coupon newsletter workflow project. Let me check the SES V2 Python code examples first: Let me continue reading the documentation: Let me continue reading about the SendEmail API: Let me now research about creating email templates in SES: Let me check the documentation about sending templated emails through Amazon SES: Let me continue reading about the templated email section: Now I'll check for more specific information about working with contact lists in Amazon SES v2: Let me check the documentation for creating contact lists in SES v2: Let's look at a complete Amazon SES v2 newsletter workflow example to understand all the required API calls together: Let me look at more of this newsletter workflow example:  Based on my research of Amazon SES v2's Python API references for this coupon newsletter workflow project, I'll summarize the key API components needed:

# Summary of Python API for Amazon SES Coupon Newsletter Workflow

## Core API Components

1. **Email Identity Management**
   - `create_email_identity`: Creates a verified email identity for sending and receiving
   - Parameters: `EmailIdentity` (the email address to verify)
   - Error handling needed for `AlreadyExistsException`

2. **Contact List Management**
   - `create_contact_list`: Creates a contact list for newsletter subscribers
   - Parameters: `ContactListName` (e.g., "CouponNewsletter")
   - Optional parameters: `Description`, `Topics` for subscription preferences
   
   - `create_contact`: Adds a subscriber to the contact list
   - Parameters: `ContactListName`, `EmailAddress`
   - Optional: `TopicPreferences`, `AttributesData` for recipient personalization
   
   - `list_contacts`: Retrieves subscribers from a contact list
   - Parameters: `ContactListName`
   - Optional: `Filter` (e.g., `FilteredStatus`: "OPT_IN")

3. **Email Template Management**
   - `create_email_template`: Creates a reusable template for coupon emails
   - Parameters: 
     - `TemplateName` (e.g., "WeeklyCouponTemplate") 
     - `TemplateContent` containing `Subject`, `Html`, `Text` with placeholders like {{variable}}

4. **Email Sending**
   - `send_email` (Simple format): For sending welcome emails
   - Parameters:
     - `FromEmailAddress`: Sender email (from environment variable)
     - `Destination`: Contains `ToAddresses` array with recipient emails
     - `Content.Simple`: Contains `Subject`, `Body.Text`, `Body.Html` for the welcome email
   
   - `send_email` (Template format): For sending templated coupon newsletters
   - Parameters:
     - `FromEmailAddress`: Sender email (from environment variable)
     - `Destination`: Contains `ToAddresses` array with recipient emails
     - `Content.Template`: Contains `TemplateName` and `TemplateData` (JSON string with template variables)
     - Optional: `ListManagementOptions.ContactListName` for tracking with the contact list

## Error Handling

The following exception types should be handled:
- `AlreadyExistsException`: When trying to create resources that already exist
- `NotFoundException`: When resources don't exist
- `LimitExceededException`: When SES limits are exceeded (e.g., too many templates)
- `TooManyRequestsException`: Rate limiting (important for email sending)
- General `ClientError` for other AWS-specific errors

## Templating

Templates can use placeholders like {{variable}} that get replaced with values provided in the `TemplateData` parameter when sending emails. For the coupon newsletter, we'll use templates to personalize offers for each recipient.

## Best Practices:

1. Wait 1-2 seconds between sending emails when in the SES sandbox environment
2. Use proper error handling for all API calls
3. Use a configuration set with Rendering Failure notifications (optional but recommended)
4. Load email content from HTML files rather than hardcoding
5. For testing, use "plus addressing" (e.g., user+test1@example.com) to send to a single base address

This API summary covers all the required components for implementing the coupon newsletter workflow, focusing on the Python (boto3) SDK.