#!/usr/bin/sh

cd $(dirname $0)

export AILLY_ENGINE=bedrock
export AWS_PROFILE=ailly.dev

npm run dev
