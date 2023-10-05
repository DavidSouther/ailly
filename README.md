# Ailly - AI Writing Ally

Load your writing.
Train Ailly on your voice.
Write your outline.
Prompt Ailly to continue to continue the writing.
Edit its output, and retrain to get it even more like that.

## Running Ailly

This is powered by [Next.js](https://nextjs.org/) using App Router.

- Clone the repo, install dependencies, and duplicate the env file for local keys.
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install ; cp .env .env.local`
- Update `.env.local` with your [OpenAI API key](https://platform.openai.com/account/api-keys).
- Clear out the `content/` folder, and replace it with your writing.
  - TODO: Provide content importers for email
  - TODO: Provide instructions for "best practices" creating
- Start the project locally with `npm run dev`
  - Default at http://localhost:3000
- Visit the [`/content`](http://localhost:3000/content) route.
  - Generate all prompts with the "Generate all" button.
    - The token count is an estimate and the pricing is advisory; there are no limits placed on token size during the API request.
- Prepare the project for deployment with `npm run export`
- Good luck!

## Rhymes with Hailey
