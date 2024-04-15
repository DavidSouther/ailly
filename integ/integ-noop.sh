#!/usr/bin/env bash

cd $(dirname $0)
AILLY_ENGINE=noop ./integ.sh
exit $?
