#!/bin/sh
set -eu

echo "==> Updating apt and installing baseline packages"
sudo apt update
sudo apt full-upgrade -y
sudo apt install -y unattended-upgrades ufw rclone curl git

echo "==> Configuring UFW"
sudo ufw allow OpenSSH
sudo ufw allow http
sudo ufw allow https
sudo ufw --force enable

echo "==> Installing Docker"
if ! command -v docker >/dev/null; then
  curl -fsSL https://get.docker.com | sudo sh
  sudo usermod -aG docker "$USER"
  echo "Log out and back in for docker group membership."
fi

echo "==> Hardening SSH"
sudo sed -i 's/^#*PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config
sudo sed -i 's/^#*PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
sudo systemctl reload ssh

echo "==> Bootstrap complete. Next: cd into the repo and run ops/deploy.sh"
