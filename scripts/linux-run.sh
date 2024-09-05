#!/usr/bin/env bash

IP=$(ifconfig en0 | grep inet | awk '$1=="inet" {print $2}')
docker run -e DISPLAY=$IP -v /tmp/.X11-unix:/tmp/.X11-unix -v .:/repo-quest --platform linux/amd64 -ti rq-linux bash