#!/bin/sh

set -e
set -x

cd $(dirname $0)

ailly --root ./root --log-level debug >log
grep -q "Ready to generate 1 messages" log

ailly --root ./root --log-level debug --max-depth 2 >log
grep -q "Ready to generate 2 messages" log

rm log
