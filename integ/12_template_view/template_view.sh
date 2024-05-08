#!/bin/sh

set -e
set -x

cd $(dirname $0)

ailly --root ./root --template-view foo.yaml file.txt >log
grep -q "FOO" ./root/file.txt.ailly.md
grep -vq "BAR" ./root/file.txt.ailly.md
grep -q "BAZ" ./root/file.txt.ailly.md
grep -q "BANG" ./root/file.txt.ailly.md
rm ./root/file.txt.ailly.md

ailly --root ./root --template-view foo.yaml --template-view bar.yaml file.txt >log
grep -q "FOO" ./root/file.txt.ailly.md
grep -q "BAR" ./root/file.txt.ailly.md
grep -q "BAZ" ./root/file.txt.ailly.md
grep -q "BANG" ./root/file.txt.ailly.md
rm ./root/file.txt.ailly.md

rm log
