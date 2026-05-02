# Developing

These are instructions on how to run various Ailly components.

## Developing ailly command line

- Clone the repo and select the `ailly_rust` branch
  - `git clone https://github.com/davidsouther/ailly.git ; cd ailly ; git switch ailly_rust`
- Set any environment variables for your engine
  - `export OPENAI_API_KEY=sk-...`
  - `export AILLY_ENGINE=bedrock` default: openai, others depending on version.
- Run ailly with `cargo run`
  - `cd content/33_dad_jokes`
  - `npx ailly .`
- Optionally, create an alias to run ailly
  - Directly with `alias ailly="$(PWD)/target/bin/ailly`
