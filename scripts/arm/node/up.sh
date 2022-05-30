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
echo -n "Please input Master IP: "
read -r MASTER_IP
export MASTER_IP=$MASTER_IP
ETCD_ENDPOINT="http://$MASTER_IP:2379"
printf 'ETCD_ENDPOINT="%s"\n' "$ETCD_ENDPOINT" > /etc/rminik8s/conf.env
DNS_IP=$MASTER_IP

# install flannel
if [ ! -f /usr/local/bin/rkubelet ]; then
  printf "Installing flanneld ...\n"
  wget -q --show-progress -O /usr/local/bin/flanneld "http://minik8s.xyz:8008/flanneld-$ARCH"
  chmod +x /usr/local/bin/flanneld
else
  printf "flanneld installed\n\n"
fi
cp ./flanneld.service /etc/systemd/system/flanneld.service
printf "flanneld service installed\n\n"

# start flannel service
systemctl daemon-reload
systemctl restart flanneld.service
while [ ! -f /run/flannel/subnet.env ]; do
  echo "Waiting for flanneld service"
  sleep 0.1
done
set -a; source /run/flannel/subnet.env; set +a
printf "flanneld service ok\n\n"

# configure dns
systemctl stop systemd-resolved
printf "nameserver $DNS_IP\n" > /etc/resolv.conf

# configure docker
envsubst < ./daemon.example.json > ./daemon.json
mv ./daemon.json /etc/docker/daemon.json
systemctl restart docker

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

# start rkube-proxy
printf "Installing rkube-proxy ...\n"
if [ ! -f /usr/local/bin/rkube-proxy ]; then
  wget -q --show-progress -O /usr/local/bin/rkube-proxy http://minik8s.xyz:8008/rkube-proxy-arm
  chmod +x /usr/local/bin/rkube-proxy
fi
cp ./rkubeproxy.service /etc/systemd/system/rkubeproxy.service
printf "ETCD_ENDPOINT=$ETCD_ENDPOINT\nAPI_SERVER_ENDPOINT=http://$MASTER_IP:8080\n" > /etc/rminik8s/node.env
systemctl daemon-reload
systemctl restart rkubeproxy.service
printf "rkube-proxy started\n\n"

printf "Installing rkubectl ...\n"
if [ ! -f /usr/local/bin/rkubectl ]; then
  wget -q --show-progress -O /usr/local/bin/rkubectl http://minik8s.xyz:8008/rkubectl-arm
  chmod +x /usr/local/bin/rkubectl
fi
printf "rkubectl installed\n\n"


