#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

printf "nameserver 119.29.29.29\n" > /etc/resolv.conf
sudo systemctl stop rkubelet.service
sudo systemctl stop rkubeproxy.service
docker rm -f cadvisor
