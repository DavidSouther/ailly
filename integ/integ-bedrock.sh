#!/usr/bin/env bash

cd $(dirname $0)
AILLY_ENGINE=bedrock ./integ.sh
exit $?
