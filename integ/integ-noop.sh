#!/usr/bin/env bash

cd $(dirname $0)
AILLY_NOOP_TIMEOUT=0 AILLY_ENGINE=noop ./integ.sh
exit $?
