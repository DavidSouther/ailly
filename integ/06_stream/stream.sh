#!/bin/bash

cd $(dirname $0)
set -x
set -e

AILLY_NOOP_STREAM=y ailly --prompt "Tell me a joke" --stream | tee out
[ -s out ]
rm out
