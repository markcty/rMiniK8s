apt install -y dnsmasq
systemctl stop systemd-resolved
systemctl disable dnsmasq
systemctl stop dnsmasq
dnsmasq -A /minik8s.com/127.0.0.1
if ! grep -Fq "minik8s" /etc/resolv.conf; then 
  mv /etc/resolv.conf /etc/resolv.conf.bk
  printf '#minik8s\nnameserver 127.0.0.1\n' | cat - /etc/resolv.conf.bk > /etc/resolv.conf
fi