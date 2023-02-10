#!/bin/bash

set -x
fly proxy -a kom-pg 15432:5432 &
sleep 5
psql -h localhost -p 15432 -U pinion
kill $!
