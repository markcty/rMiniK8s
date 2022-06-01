#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

printf "nameserver 119.29.29.29\n" > /etc/resolv.conf

docker rm -f etcd
docker rm -f prometheus
systemctl daemon-reload
docker rm -f dns
docker rm -f cadvisor
