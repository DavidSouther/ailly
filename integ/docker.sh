#!/bin/sh

cd "$(dirname $0)"

VERSION=$(cat ../cli/package.json | jq '.version')

docker run --interactive --tty \
  --volume ~/.aws:/root/.aws --env AWS_PROFILE="$AWS_PROFILE" --env OPENAI_API_KEY="${OPENAI_API_KEY}" \
  --volume .:/content --workdir /content \
  node:alpine \
  /bin/sh -c "npm install --global @ailly/cli@${VERSION} ; ailly --prompt 'Tell me a joke' "
