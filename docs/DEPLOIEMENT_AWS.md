# Guide de Déploiement AWS - Polymarket Trading Bot

Ce guide explique comment déployer le bot sur un VPS AWS pour contourner les restrictions géographiques et avoir un bot qui tourne 24/7.

## Table des matières

1. [Architecture](#1-architecture)
2. [Créer une instance AWS EC2](#2-créer-une-instance-aws-ec2)
3. [Configurer le serveur](#3-configurer-le-serveur)
4. [Installer les dépendances](#4-installer-les-dépendances)
5. [Déployer le bot](#5-déployer-le-bot)
6. [Paper Trading vs Live Trading](#6-paper-trading-vs-live-trading)
7. [Configurer l'accès web à l'interface](#7-configurer-laccès-web-à-linterface)
8. [Sécuriser le serveur](#8-sécuriser-le-serveur)
9. [Monitoring et maintenance](#9-monitoring-et-maintenance)
10. [Commandes utiles](#10-commandes-utiles)
11. [Dépannage](#11-dépannage)

---

## 1. Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         AWS EC2 (Singapore)                      │
│                                                                  │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────┐ │
│  │  Rust Backend   │◄──►│  Vite Frontend  │◄──►│   Nginx     │ │
│  │  (trading-bot)  │    │  (port 5173)    │    │  (port 80)  │ │
│  └─────────────────┘    └─────────────────┘    └─────────────┘ │
│          │                                            │         │
│          ▼                                            │         │
│  ┌─────────────────┐                                 │         │
│  │   Polymarket    │                                 │         │
│  │      API        │                                 │         │
│  └─────────────────┘                                 │         │
│                                                       │         │
└───────────────────────────────────────────────────────┼─────────┘
                                                        │
                                                        ▼
                                              ┌─────────────────┐
                                              │   Ton PC/Mac    │
                                              │   (Navigateur)  │
                                              └─────────────────┘
```

**Avantages de cette architecture :**
- Bot tourne 24/7 sans ton PC
- Accès à l'interface depuis n'importe où
- IP dans un pays autorisé (Singapore, UK, etc.)
- Latence optimale vers les serveurs Polymarket

---

## 2. Créer une instance AWS EC2

### 2.1 Prérequis

- Compte AWS (https://aws.amazon.com)
- Carte bancaire pour la facturation

### 2.2 Choisir la région

**Régions recommandées** (pas de restrictions Polymarket) :

| Région | Code AWS | Latence vers Polymarket |
|--------|----------|------------------------|
| Singapore | `ap-southeast-1` | ~150ms |
| Tokyo | `ap-northeast-1` | ~180ms |
| London | `eu-west-2` | ~50ms |
| Canada | `ca-central-1` | ~80ms |

⚠️ **Éviter** : `us-east-1`, `us-west-2`, `eu-west-3` (Paris)

### 2.3 Créer l'instance

1. **Aller dans EC2** : https://console.aws.amazon.com/ec2

2. **Cliquer sur "Launch Instance"**

3. **Configurer l'instance :**

   ```
   Name: polymarket-bot

   AMI: Ubuntu Server 22.04 LTS (HVM), SSD Volume Type

   Instance type: t3.small (2 vCPU, 2 GB RAM)
   - Pour commencer : t3.micro (Free tier, 1 GB RAM)
   - Recommandé : t3.small (~15$/mois)
   - Performance : t3.medium (~30$/mois)

   Key pair: Créer une nouvelle paire de clés
   - Name: polymarket-key
   - Type: RSA
   - Format: .pem (Linux/Mac) ou .ppk (Windows/PuTTY)
   - TÉLÉCHARGER ET SAUVEGARDER CE FICHIER !

   Network settings:
   - Allow SSH traffic: ✅ (port 22)
   - Allow HTTPS traffic: ✅ (port 443)
   - Allow HTTP traffic: ✅ (port 80)

   Storage: 20 GB gp3
   ```

4. **Cliquer sur "Launch instance"**

5. **Noter l'IP publique** de l'instance (ex: `63.35.188.77`)

### 2.4 Configurer le Security Group

#### ⚠️ Recommandation de sécurité

AWS affiche un avertissement si vous utilisez `0.0.0.0/0` (toutes les IPs) :
> *Les règles avec la source 0.0.0.0/0 autorisent toutes les adresses IP à accéder à votre instance.*

**Pour un bot de trading personnel, restreignez TOUS les accès à votre IP uniquement.**

#### Configuration recommandée (sécurisée)

| Type | Port | Source | Description |
|------|------|--------|-------------|
| SSH | 22 | Mon IP | Accès SSH administration |
| HTTP | 80 | Mon IP | Interface web |
| HTTPS | 443 | Mon IP | Interface web sécurisée |
| Custom TCP | 3000 | Mon IP | API Backend |

Dans AWS, sélectionnez **"Mon IP"** dans le menu déroulant "Source" - AWS remplira automatiquement votre adresse IP actuelle.

#### ⚠️ Attention : IP dynamique

Si vous avez une connexion internet résidentielle (Livebox, Freebox, etc.), votre IP peut changer périodiquement.

**Solutions si votre IP change :**

1. **Mettre à jour le Security Group** quand votre IP change :
   - Console AWS → EC2 → Security Groups → Sélectionner le groupe → "Edit inbound rules"
   - Modifier la source avec votre nouvelle IP

2. **Utiliser une plage IP de votre FAI** (moins sécurisé) :
   - Exemple : `86.xxx.0.0/16` au lieu d'une IP unique

3. **Utiliser un VPN avec IP fixe** :
   - Certains VPN offrent une IP statique dédiée

---

## 3. Configurer le serveur

### 3.1 Se connecter en SSH

**Linux/Mac :**
```bash
# Protéger la clé
chmod 400 polymarket-key.pem

# Se connecter
ssh -i polymarket-key.pem ubuntu@63.35.188.77
```

**Windows (CMD) :**

⚠️ **Important** : Sur Windows, vous devez d'abord corriger les permissions de la clé privée. SSH refuse de fonctionner si d'autres utilisateurs ont accès au fichier.

```cmd
:: Se placer dans le dossier de la clé
cd "E:\developpement\conf\pair de cle aws"

:: Supprimer l'héritage des permissions
icacls "ubuntu-poly-bot-key.pem" /inheritance:r

:: Supprimer les groupes système qui ont accès
icacls "ubuntu-poly-bot-key.pem" /remove:g "BUILTIN\Administrators"
icacls "ubuntu-poly-bot-key.pem" /remove:g "BUILTIN\Users"
icacls "ubuntu-poly-bot-key.pem" /remove:g "NT AUTHORITY\SYSTEM"

:: Donner uniquement à votre utilisateur le droit de lecture
:: Remplacez "ravet" par votre nom d'utilisateur Windows
icacls "ubuntu-poly-bot-key.pem" /grant:r "ravet:(R)"

:: Vérifier les permissions (doit afficher uniquement votre utilisateur)
icacls "ubuntu-poly-bot-key.pem"
:: Résultat attendu : ubuntu-poly-bot-key.pem VOTRE-PC\ravet:(R)

:: Se connecter
ssh -i "ubuntu-poly-bot-key.pem" ubuntu@63.35.188.77
```

**Note** : Si le chemin contient des espaces, utilisez des guillemets autour du chemin complet :
```cmd
ssh -i "E:\developpement\conf\pair de cle aws\ubuntu-poly-bot-key.pem" ubuntu@63.35.188.77
```

**Windows (PuTTY) - Alternative :**
1. Convertir .pem en .ppk avec PuTTYgen
2. Configurer PuTTY avec l'IP et la clé

### 3.2 Mettre à jour le système

```bash
sudo apt update && sudo apt upgrade -y
sudo reboot
```

Attendre 1 minute puis se reconnecter.

---

## 4. Installer les dépendances

### 4.1 Installer les outils de base

```bash
sudo apt install -y build-essential pkg-config libssl-dev git curl wget unzip
```

### 4.2 Installer Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# Vérifier
rustc --version
cargo --version
```

### 4.3 Installer Node.js (v20 LTS)

```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs

# Vérifier
node --version
npm --version
```

### 4.4 Installer Nginx

```bash
sudo apt install -y nginx
sudo systemctl enable nginx
sudo systemctl start nginx
```

### 4.5 Installer PM2 (Process Manager)

```bash
sudo npm install -g pm2
```

---

## 5. Déployer le bot

### Option A : Déploiement automatique avec GitHub Actions (Recommandé)

Le projet inclut un workflow GitHub Actions qui compile et déploie automatiquement à chaque push sur `main`.

#### 5.A.1 Configurer les secrets GitHub

Dans ton repo GitHub, aller dans **Settings → Secrets and variables → Actions** et ajouter :

| Secret | Description | Exemple |
|--------|-------------|---------|
| `AWS_SSH_PRIVATE_KEY` | Contenu complet du fichier .pem | `-----BEGIN RSA PRIVATE KEY-----...` |
| `AWS_HOST` | IP publique de l'instance EC2 | `63.35.188.77` |

Pour `AWS_SSH_PRIVATE_KEY`, copier tout le contenu du fichier .pem (incluant les lignes BEGIN/END).

#### 5.A.2 Installer PM2 sur le serveur (première fois uniquement)

```bash
# Se connecter au serveur
ssh -i "polymarket-key.pem" ubuntu@63.35.188.77

# Installer PM2 globalement
sudo npm install -g pm2

# Configurer PM2 pour démarrer au boot
pm2 startup
# Exécuter la commande affichée (sudo env PATH=...)
pm2 save
```

#### 5.A.3 Configurer le fichier .env sur le serveur

```bash
mkdir -p ~/poly_bot
nano ~/poly_bot/.env
```

Ajouter la configuration (voir section 5.2 ci-dessous pour le contenu).

#### 5.A.4 Déclencher le déploiement

Le déploiement se déclenche automatiquement à chaque push sur `main` :

```bash
git add .
git commit -m "Mon commit"
git push origin main
```

Ou manuellement depuis GitHub : **Actions → Build and Deploy → Run workflow**

#### 5.A.5 Vérifier le déploiement

```bash
# Sur le serveur, vérifier que le service tourne
pm2 status
pm2 logs poly-bot

# Vérifier que le port 3000 écoute
ss -tlnp | grep 3000

# Tester l'API
curl http://localhost:3000/api/status
```

#### 5.A.6 Accéder à l'interface

Ouvrir dans ton navigateur :
```
http://63.35.188.77:3000
```

Le serveur Rust sert à la fois :
- L'interface web (frontend) sur `/`
- L'API sur `/api/*`

---

### Option B : Déploiement manuel

#### 5.B.1 Cloner le projet

```bash
cd ~
git clone https://github.com/TON_USERNAME/poly_bot.git
cd poly_bot
```

**Ou transférer depuis ton PC :**
```bash
# Depuis ton PC local
scp -i polymarket-key.pem -r ./poly_bot ubuntu@63.35.188.77:~/
```

### 5.B.2 Configurer l'environnement

```bash
cd ~/poly_bot

# Créer le fichier .env
nano .env
```

Coller le contenu suivant (avec tes vraies valeurs) :

```env
# POLYMARKET TRADING BOT CONFIGURATION

# WALLET CONFIGURATION
POLY_PROXY_WALLET=0x1d3cf6c3f7e609f90fb55f22bb88c5982aec6838
POLY_PRIVATE_KEY=0x_TA_CLE_PRIVEE_ICI

# BUILDER PROGRAM (Gasless Trading)
POLY_BUILDER_API_KEY=ton_api_key
POLY_BUILDER_API_SECRET=ton_secret
POLY_BUILDER_API_PASSPHRASE=ton_passphrase

# RPC CONFIGURATION
POLY_RPC_URL=https://polygon-rpc.com
POLY_CHAIN_ID=137

# API CONFIGURATION
POLY_CLOB_HOST=https://clob.polymarket.com

# APPLICATION SETTINGS
POLY_DATA_DIR=credentials
POLY_LOG_LEVEL=INFO

# RUST LOGGING
RUST_LOG=info,trading_engine=debug
```

Sauvegarder : `Ctrl+X`, puis `Y`, puis `Enter`

### 5.B.3 Installer les dépendances Node.js

```bash
npm install
```

### 5.B.4 Compiler le backend Rust

```bash
cargo build --release
```

Cela prend ~5-10 minutes la première fois.

### 5.B.5 Tester le bot en CLI

```bash
# Tester la configuration
./target/release/poly-cli config

# Tester la connexion (récupérer un orderbook)
./target/release/poly-cli orderbook TOKEN_ID_TEST
```

---

## 6. Paper Trading vs Live Trading

### 6.1 Comprendre les modes de trading

Le bot supporte deux modes de fonctionnement :

| Mode | Description | Risque |
|------|-------------|--------|
| **Paper Trading** | Simulation avec argent virtuel | Aucun risque |
| **Live Trading** | Trading réel sur Polymarket | Argent réel en jeu |

⚠️ **IMPORTANT** : Par défaut, le bot démarre en **Paper Trading** pour éviter toute perte accidentelle d'argent réel.

### 6.2 Configuration Paper Trading

Dans le fichier `~/poly_bot/.env`, ajoutez ces variables :

```env
# ===========================================
# PAPER TRADING CONFIGURATION
# ===========================================

# Enable/disable paper trading (true = simulation, false = real money)
POLY_PAPER_TRADING=true

# Initial virtual balance in USDC for paper trading
POLY_PAPER_INITIAL_BALANCE=10000

# Slippage model: "none", "realistic", "conservative"
# - none: fills at exact order price (optimistic)
# - realistic: based on orderbook depth (recommended)
# - conservative: adds extra 0.5% slippage penalty
POLY_PAPER_SLIPPAGE=realistic

# Allow partial fills in simulation
POLY_PAPER_PARTIAL_FILLS=true

# Simulated fee rate in basis points (100 bps = 1%)
POLY_PAPER_FEE_BPS=0
```

### 6.3 Démarrer en Paper Trading (par défaut)

```bash
# Avec PM2
pm2 start ~/poly_bot/poly-cli --name poly-bot -- serve --port 3000 --host 0.0.0.0

# Ou directement
./poly-cli serve --port 3000 --host 0.0.0.0
```

Le serveur affichera :
```
╔══════════════════════════════════════════════════════════════════╗
║                    📝 PAPER TRADING MODE                         ║
╠══════════════════════════════════════════════════════════════════╣
║  Orders will be SIMULATED - no real money at risk.               ║
╚══════════════════════════════════════════════════════════════════╝
```

### 6.4 Passer en Live Trading (argent réel)

⚠️ **ATTENTION** : En mode Live Trading, vous pouvez perdre de l'argent réel !

**Étape 1 : Modifier la configuration**

```bash
nano ~/poly_bot/.env
```

Changez :
```env
POLY_PAPER_TRADING=false
```

**Étape 2 : Redémarrer avec le flag `--live`**

Le flag `--live` est **obligatoire** pour confirmer que vous acceptez les risques.

```bash
# Arrêter le service actuel
pm2 stop poly-bot

# Démarrer en mode live (avec acknowledgment explicite)
pm2 start ~/poly_bot/poly-cli --name poly-bot -- serve --port 3000 --host 0.0.0.0 --live
```

Le serveur affichera un avertissement :
```
╔══════════════════════════════════════════════════════════════════╗
║            ⚠️  WARNING: LIVE TRADING MODE ⚠️                      ║
╠══════════════════════════════════════════════════════════════════╣
║  Paper trading is DISABLED!                                      ║
║  All orders will be sent to the REAL Polymarket API.             ║
║  You can LOSE REAL MONEY!                                        ║
╚══════════════════════════════════════════════════════════════════╝
```

### 6.5 Revenir en Paper Trading

```bash
# Modifier .env
nano ~/poly_bot/.env
# Remettre: POLY_PAPER_TRADING=true

# Redémarrer (pas besoin du flag --live)
pm2 restart poly-bot
```

### 6.6 Vérifier le mode actuel

Via l'API :
```bash
curl http://localhost:3000/api/status
```

Réponse en Paper Trading :
```json
{
  "mode": "paper",
  "is_paper_trading": true,
  "paper_trading": {
    "enabled": true,
    "initial_balance": "10000",
    "slippage_model": "realistic"
  }
}
```

---

## 7. Configurer l'accès web à l'interface

### 7.1 Créer le build de production du frontend

```bash
cd ~/poly_bot
npm run build
```

Cela crée un dossier `dist/` avec les fichiers statiques.

### 7.2 Configurer Nginx

```bash
sudo nano /etc/nginx/sites-available/polymarket-bot
```

Coller cette configuration :

```nginx
server {
    listen 80;
    server_name _;  # Remplacer par ton domaine si tu en as un

    # Frontend statique
    root /home/ubuntu/poly_bot/dist;
    index index.html;

    # Compression
    gzip on;
    gzip_types text/plain text/css application/json application/javascript;

    # Frontend SPA routing
    location / {
        try_files $uri $uri/ /index.html;
    }

    # API Backend proxy (si nécessaire)
    location /api/ {
        proxy_pass http://127.0.0.1:3000/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_cache_bypass $http_upgrade;
    }

    # WebSocket support
    location /ws/ {
        proxy_pass http://127.0.0.1:3000/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }

    # Sécurité basique
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
}
```

Activer le site :

```bash
sudo ln -s /etc/nginx/sites-available/polymarket-bot /etc/nginx/sites-enabled/
sudo rm /etc/nginx/sites-enabled/default
sudo nginx -t
sudo systemctl reload nginx
```

### 7.3 Créer un service API backend

Créer un fichier pour le service backend :

```bash
sudo nano /etc/systemd/system/polymarket-bot.service
```

Contenu :

```ini
[Unit]
Description=Polymarket Trading Bot Backend
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/poly_bot
Environment=PATH=/home/ubuntu/.cargo/bin:/usr/local/bin:/usr/bin:/bin
EnvironmentFile=/home/ubuntu/poly_bot/.env
ExecStart=/home/ubuntu/poly_bot/target/release/poly-cli serve
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**Note :** Il faudra peut-être créer une commande `serve` dans le CLI pour exposer une API HTTP. Sinon, utiliser PM2 pour le frontend.

### 7.4 Alternative : Lancer avec PM2

```bash
cd ~/poly_bot

# Lancer le frontend en mode développement (avec accès au backend)
pm2 start npm --name "poly-frontend" -- run dev -- --host 0.0.0.0

# Ou lancer en mode preview (production)
pm2 start npm --name "poly-frontend" -- run preview -- --host 0.0.0.0

# Sauvegarder la configuration PM2
pm2 save
pm2 startup
```

### 7.5 Accéder à l'interface

Ouvrir dans ton navigateur :

```
http://63.35.188.77
```

(Remplacer par l'IP de ton instance)

---

## 8. Sécuriser le serveur

### 8.1 Configurer un mot de passe pour l'interface (optionnel)

```bash
# Créer un fichier de mot de passe
sudo apt install apache2-utils
sudo htpasswd -c /etc/nginx/.htpasswd admin
```

Ajouter dans la config Nginx :

```nginx
location / {
    auth_basic "Polymarket Bot";
    auth_basic_user_file /etc/nginx/.htpasswd;
    try_files $uri $uri/ /index.html;
}
```

### 8.2 Configurer HTTPS avec Let's Encrypt

Si tu as un nom de domaine :

```bash
sudo apt install certbot python3-certbot-nginx
sudo certbot --nginx -d ton-domaine.com
```

### 8.3 Configurer le firewall

Le firewall UFW sur le serveur complète le Security Group AWS :

```bash
sudo ufw allow 22/tcp    # SSH
sudo ufw allow 80/tcp    # HTTP
sudo ufw allow 443/tcp   # HTTPS
sudo ufw allow 3000/tcp  # API Backend
sudo ufw enable
```

**Note :** Le Security Group AWS filtre déjà par IP. Le firewall UFW est une couche de protection supplémentaire.

### 8.4 Fail2ban (protection SSH)

```bash
sudo apt install fail2ban
sudo systemctl enable fail2ban
sudo systemctl start fail2ban
```

---

## 9. Monitoring et maintenance

### 9.1 Voir les logs

```bash
# Logs PM2
pm2 logs poly-frontend

# Logs système
sudo journalctl -u polymarket-bot -f

# Logs Nginx
sudo tail -f /var/log/nginx/access.log
sudo tail -f /var/log/nginx/error.log
```

### 9.2 Monitoring avec PM2

```bash
# Status
pm2 status

# Monitoring temps réel
pm2 monit

# Métriques
pm2 show poly-frontend
```

### 9.3 Mettre à jour le bot

```bash
cd ~/poly_bot

# Arrêter
pm2 stop poly-frontend

# Mettre à jour
git pull origin master

# Recompiler
cargo build --release
npm run build

# Redémarrer
pm2 restart poly-frontend
```

### 9.4 Sauvegardes automatiques

```bash
# Créer un script de backup
nano ~/backup.sh
```

```bash
#!/bin/bash
DATE=$(date +%Y%m%d)
tar -czf ~/backups/poly_bot_$DATE.tar.gz ~/poly_bot/.env ~/poly_bot/credentials/
# Garder seulement les 7 derniers backups
ls -t ~/backups/*.tar.gz | tail -n +8 | xargs -r rm
```

```bash
chmod +x ~/backup.sh
mkdir -p ~/backups

# Ajouter au cron (tous les jours à 2h)
crontab -e
# Ajouter: 0 2 * * * /home/ubuntu/backup.sh
```

---

## 10. Commandes utiles

### Gestion du bot

```bash
# Démarrer
pm2 start poly-frontend

# Arrêter
pm2 stop poly-frontend

# Redémarrer
pm2 restart poly-frontend

# Voir les logs
pm2 logs poly-frontend --lines 100
```

### Gestion du serveur

```bash
# Redémarrer Nginx
sudo systemctl restart nginx

# Vérifier l'utilisation des ressources
htop

# Espace disque
df -h

# Mémoire
free -m
```

### Debugging

```bash
# Tester la config Nginx
sudo nginx -t

# Vérifier les ports ouverts
sudo netstat -tlnp

# Tester la connexion Polymarket depuis le VPS
curl -I https://clob.polymarket.com/time
```

---

## 11. Dépannage

### Le site ne charge pas

```bash
# Vérifier que Nginx tourne
sudo systemctl status nginx

# Vérifier les logs d'erreur
sudo tail -f /var/log/nginx/error.log

# Vérifier que le port 80 est ouvert
sudo ufw status
```

### Le bot ne se connecte pas à Polymarket

```bash
# Tester depuis le VPS
curl https://clob.polymarket.com/time

# Si ça ne marche pas, l'IP est peut-être bloquée
# Essayer une autre région AWS
```

### Erreur de compilation Rust

```bash
# Mettre à jour Rust
rustup update

# Nettoyer et recompiler
cargo clean
cargo build --release
```

### PM2 ne démarre pas au boot

```bash
pm2 startup
pm2 save
```

### GitHub Actions échoue

**Erreur : "AWS_SSH_PRIVATE_KEY secret is not set"**
- Vérifier que le secret est bien configuré dans GitHub → Settings → Secrets
- Le nom doit être exactement `AWS_SSH_PRIVATE_KEY`

**Erreur : "SSH connection failed"**
- Vérifier que le Security Group autorise le port 22 depuis `0.0.0.0/0` (GitHub Actions utilise des IPs dynamiques)
- Ou temporairement : autoriser toutes les IPs pour le déploiement, puis restreindre après

**Le service ne démarre pas après déploiement**
```bash
# Vérifier si PM2 est installé
pm2 --version

# Si non installé
sudo npm install -g pm2

# Vérifier les logs
pm2 logs poly-bot --lines 50
```

### L'interface ne s'affiche pas (404 ou page blanche)

```bash
# Vérifier que le dossier dist existe
ls -la ~/poly_bot/dist/

# Vérifier les logs du serveur
pm2 logs poly-bot

# Redémarrer le service
pm2 restart poly-bot
```

---

## Coûts estimés AWS

| Service | Coût mensuel |
|---------|--------------|
| EC2 t3.micro (Free tier 1ère année) | $0 |
| EC2 t3.small | ~$15 |
| EC2 t3.medium | ~$30 |
| Stockage 20GB | ~$2 |
| Transfert données | ~$1-5 |
| **Total estimé** | **$15-40/mois** |

---

## Alternatives moins chères

| Fournisseur | Prix | Specs |
|-------------|------|-------|
| Hetzner Cloud | 4€/mois | 2 vCPU, 2GB RAM |
| DigitalOcean | $6/mois | 1 vCPU, 1GB RAM |
| Vultr | $6/mois | 1 vCPU, 1GB RAM |
| Linode | $5/mois | 1 vCPU, 1GB RAM |

Ces alternatives sont souvent suffisantes et moins chères qu'AWS.

---

## Checklist de déploiement

### Déploiement automatique (GitHub Actions)

- [ ] Instance EC2 créée dans une région autorisée
- [ ] Security Group configuré (ports 22, 80, 443, 3000)
- [ ] SSH fonctionnel
- [ ] Node.js installé sur le serveur
- [ ] PM2 installé (`sudo npm install -g pm2`)
- [ ] PM2 startup configuré (`pm2 startup` + commande affichée)
- [ ] Secrets GitHub configurés (`AWS_SSH_PRIVATE_KEY`, `AWS_HOST`)
- [ ] Fichier .env configuré sur le serveur (`~/poly_bot/.env`)
- [ ] Premier déploiement lancé (push sur main)
- [ ] Interface accessible via `http://IP:3000`
- [ ] API fonctionnelle (`curl http://IP:3000/api/status`)

### Déploiement manuel (optionnel)

- [ ] Rust installé sur le serveur
- [ ] Projet cloné/transféré
- [ ] Backend compilé (`cargo build --release`)
- [ ] Frontend buildé (`npm run build`)
- [ ] Nginx configuré (si besoin d'un reverse proxy)

### Sécurité (recommandé)

- [ ] ⚠️ Security Group : sources limitées à "Mon IP" (sauf port 22 pour GitHub Actions)
- [ ] HTTPS configuré (optionnel)
- [ ] Mot de passe configuré (optionnel)
- [ ] Backups configurés
- [ ] Monitoring activé

---

*Document créé le 2 février 2025*
*Dernière mise à jour : 4 février 2025 - Ajout déploiement GitHub Actions*
