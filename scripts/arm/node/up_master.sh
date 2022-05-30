#!/bin/bash
if [ "$EUID" -ne 0 ]
  then echo "Please run as root"
  exit
fi

mkdir -p /etc/rminik8s
ARCH=$(dpkg --print-architecture)
printf "ARCH: $ARCH\n"
export IP=$(ip route get 114.114.114.114 | awk '{ print $7; exit }')
printf "IP: $IP\n\n"

# get config
export MASTER_IP=$IP
ETCD_ENDPOINT="http://$MASTER_IP:2379"
printf 'ETCD_ENDPOINT="%s"\n' "$ETCD_ENDPOINT" > /etc/rminik8s/conf.env
DNS_IP=$MASTER_IP

# run cadvisor
docker run \
  --volume=/:/rootfs:ro \
  --volume=/var/run:/var/run:rw \
  --volume=/sys:/sys:ro \
  --volume=/var/lib/docker/:/var/lib/docker:ro \
  --publish=8090:8080 \
  --detach=true \
  --name=cadvisor \
  zcube/cadvisor:latest

# run rkubelet
if [ ! -f /usr/local/bin/rkubelet ]; then
  wget -q --show-progress -O /usr/local/bin/rkubelet http://minik8s.xyz:8008/rkubelet-arm
  chmod +x /usr/local/bin/rkubelet
fi
mkdir -p /var/lib/rkubelet
envsubst < ./rkubelet.yaml > /var/lib/rkubelet/config.yaml
cp ./rkubelet.service /etc/systemd/system/rkubelet.service
systemctl daemon-reload
systemctl restart rkubelet.service
printf "rkubelet service started\n\n"
