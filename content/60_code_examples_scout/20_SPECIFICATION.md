---
combined: true
debug:
  engine: bedrock
  finish: end_turn
  id: ""
  lastRun: 2025-06-01T22:31:47.178Z
  model: us.anthropic.claude-3-7-sonnet-20250219-v1:0
  region: us-east-1
prompt: |
  Write the SPECIFICATION.md for the workflow.

  The specification is independent of any programming language, and should enable any programmer competent with programming using any published AWS SDK to follow along. It must specify the API calls to make, and it must include the parameters to send. It should describe the parameters in a list format. Implementations will use the specific SDKs, so it does not need to specify URL calls to make, only the API calls and the request parameters to include. It must specify the exact environment variable names and files to use when referring to runtime data.

  Additional instructions for each section of the specification:

  Prepare the Application:
  - Print a short intro message.
  - The verified email should be provided by the builder in an environment variable for the workflow.
  - The contact list should have a hardcoded name relevant to the workflow.
  - The newsletter should use a provided template, but read it in at runtime.

  Gather Subscriber Email Addresses:

  - The application should ask the user for several (3 to 5) email addresses interactively on the command line.
  - Suggest that during testing, users use a single email address and provide a number of plus addresses.
  - The contents of the welcome email should be provided in the spec, but read from an html file at runtime.

  Send the Coupon Newsletter:

  - After gathering the subscriber emails, send a weekly newsletter.
  - Specify six coupon items for the newsletter.
  - Include links to the template reference documentation, https://docs.aws.amazon.com/ses/latest/dg/send-personalized-email-api.html

  Monitor and Review:
  - Leave this section vague for now, we will fill it in later.
---
# Coupon Newsletter Workflow Specification

## Overview

This specification outlines the steps to create and run a coupon newsletter workflow using Amazon SES V2. The workflow verifies email identities, creates a contact list, adds subscribers, sends welcome emails, and distributes coupon newsletters using templates.

## Environment Variables

- `SENDER_EMAIL`: The verified email address used to send newsletters
- `REPLY_TO_EMAIL`: The email address for recipients to reply to (can be the same as `SENDER_EMAIL`)

## Required Files

- `welcome_template.html`: HTML content for the welcome email
- `coupon_template.html`: HTML template with placeholders for the coupon newsletter

## Implementation Steps

### 1. Prepare the Application

1. Print an introductory message:
   ```
   Welcome to the Amazon SES Coupon Newsletter Workflow!
   Setting up the newsletter infrastructure...
   ```

2. Verify the sender email identity:
   - API Call: `CreateEmailIdentity`
   - Parameters:
     - `EmailIdentity`: Use the value from `SENDER_EMAIL` environment variable

3. Create a contact list for newsletter subscribers:
   - API Call: `CreateContactList`
   - Parameters:
     - `ContactListName`: "CouponNewsletter"
     - `Description`: "Subscribers to weekly coupon newsletter"
     - `Topics`: [{"TopicName": "Weekly", "DisplayName": "Weekly Coupon Deals", "Description": "Get the best deals every week", "DefaultSubscriptionStatus": "OPT_IN"}]

### 2. Gather Subscriber Email Addresses

1. Request email addresses from the command line:
   ```
   Please enter 3-5 email addresses to subscribe to the newsletter.
   During testing, you can use a single email with "+" addressing
   (e.g., your+test1@example.com, your+test2@example.com)
   ```

2. For each email address entered:
   - API Call: `CreateContact`
   - Parameters:
     - `ContactListName`: "CouponNewsletter"
     - `EmailAddress`: [user-entered email address]
     - `TopicPreferences`: [{"TopicName": "Weekly", "SubscriptionStatus": "OPT_IN"}]
     - `AttributesData`: '{"FirstName":"Valued","LastName":"Customer"}'

3. Send welcome email to each new subscriber:
   - API Call: `SendEmail` (Simple format)
   - Parameters:
     - `FromEmailAddress`: Use the value from `SENDER_EMAIL` environment variable
     - `ReplyToAddresses`: [Use the value from `REPLY_TO_EMAIL` environment variable]
     - `Destination`: {"ToAddresses": [newly added email address]}
     - `Content`: 
       - `Simple`: 
         - `Subject`: {"Data": "Welcome to our Coupon Newsletter!", "Charset": "UTF-8"}
         - `Body`: 
           - `Html`: {"Data": [content from welcome_template.html], "Charset": "UTF-8"}
           - `Text`: {"Data": "Thank you for subscribing to our coupon newsletter! You'll receive weekly deals and special offers.", "Charset": "UTF-8"}

### 3. Send the Coupon Newsletter

1. Create a template for the coupon newsletter:
   - API Call: `CreateEmailTemplate`
   - Parameters:
     - `TemplateName`: "WeeklyCouponTemplate"
     - `SubjectPart`: "This Week's Special Offers Just for You!"
     - `HtmlPart`: [content from coupon_template.html]
     - `TextPart`: "Check out this week's special offers and coupons!"

2. Define six coupon items to include in the newsletter:
   ```json
   [
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
     },
     {
       "store": "FreshGrocer", 
       "discount": "Buy one get one free",
       "code": "BOGOF",
       "expiry": "3 days"
     },
     {
       "store": "StyleShop", 
       "discount": "30% off any item",
       "code": "STYLE30",
       "expiry": "5 days"
     },
     {
       "store": "PetPlace", 
       "discount": "15% off all pet food",
       "code": "PETFOOD15",
       "expiry": "10 days"
     },
     {
       "store": "BookCorner", 
       "discount": "Buy 2 books, get 1 free",
       "code": "BOOK3FOR2",
       "expiry": "14 days"
     }
   ]
   ```

3. Retrieve all contacts from the list:
   - API Call: `ListContacts`
   - Parameters:
     - `ContactListName`: "CouponNewsletter"
     - `Filter`: {"FilteredStatus": "OPT_IN"}
     - `PageSize`: 50

4. Send the templated newsletter to each contact:
   - API Call: `SendEmail` (Template format)
   - Parameters:
     - `FromEmailAddress`: Use the value from `SENDER_EMAIL` environment variable
     - `ReplyToAddresses`: [Use the value from `REPLY_TO_EMAIL` environment variable]
     - `Destination`: {"ToAddresses": [contact's email address]}
     - `Content`: 
       - `Template`: 
         - `TemplateName`: "WeeklyCouponTemplate"
         - `TemplateData`: JSON string containing the coupon data (formatted appropriately for template)

### 4. Monitor and Review

After completing the workflow, advise users to:

1. Check the Amazon SES console for metrics on delivery, open rates, and click-through rates
2. Monitor bounce and complaint rates
3. Review analytics to optimize future newsletters

## Additional Information

For information on creating and using email templates in Amazon SES, refer to:
https://docs.aws.amazon.com/ses/latest/dg/send-personalized-email-api.html

Template placeholders should follow the format outlined in the documentation, using `{{variable}}` syntax for dynamic content insertion.