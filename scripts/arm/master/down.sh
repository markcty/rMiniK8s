#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

export IP=$(ip route get 114.114.114.114 | awk '{ print $7; exit }')
export API_SERVER_IP=${SUBNET_BASE}.100
export INGRESS_IP=${SUBNET_BASE}.101
export SERVERLESS_ROUTER_IP=${SUBNET_BASE}.102
export PROMETHEUS_IP=${SUBNET_BASE}.103
export ETCD_ENDPOINT=http://${IP}:2379
set -a; source /run/flannel/subnet.env; set +a

printf "nameserver 119.29.29.29\n" > /etc/resolv.conf

docker-compose -p minik8s-control-plane down -t 1 -v --remove-orphans 
docker rm -f etcd
systemctl daemon-reload
systemctl stop rkubeproxy
systemctl stop flanneld
systemctl stop coredns
