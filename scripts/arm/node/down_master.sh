#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

sudo systemctl stop rkubelet.service
docker rm -f cadvisor
