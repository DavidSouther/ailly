#!/bin/bash

set -x
set -e

cd $(dirname $0)

ailly --context none --prompt "Tell me a joke" >out.txt
[ -f out.txt ]
grep -q "Tell me a joke" out.txt
grep -vq '{"name":"@ailly/core"' out.txt
rm out.txt

(
  cat code.js
  echo "Explain this code"
) | ailly --context none --prompt - >out.txt

[ -f out.txt ]
grep -vq '{"name":"@ailly/core"' out.txt
grep -q "Explain this code" out.txt
rm out.txt
