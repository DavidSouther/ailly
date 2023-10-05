# AIlly - AI Writing Ally.

Load your writing.
Train your AIlly on your voice.
Write your outline.
Prompt your AIlly to continue to continue the writing.
Edit its output, and retrain to get it even more like that.

## Running Your AIlly

This is powered by [Next.js](https://nextjs.org/) using App Router.

- Clone the repo.
- Clear out the `content/` folder, and replace it with your writing.
  - TODO: Provide content importers for email
  - TODO: Provide instructions for "best practices" creating
- Update .env.local with your OpenAI API key
- Start the project locally with `npm run dev`
- Visit the `/content` route
- Prepare the project for deployment with `npm run export`
- Good luck!
