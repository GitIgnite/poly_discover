# Guide d'obtention des clés Polymarket

**Date :** Janvier 2025
**Objectif :** Configurer les clés API et wallet pour le trading bot Polymarket

---

## Table des matières

1. [Vue d'ensemble des clés nécessaires](#1-vue-densemble-des-clés-nécessaires)
2. [Créer un compte Polymarket](#2-créer-un-compte-polymarket)
3. [Récupérer l'adresse Proxy Wallet](#3-récupérer-ladresse-proxy-wallet)
4. [Obtenir les Builder API Keys](#4-obtenir-les-builder-api-keys)
5. [Configurer la clé privée](#5-configurer-la-clé-privée)
6. [Créer le fichier .env](#6-créer-le-fichier-env)
7. [Vérification de la configuration](#7-vérification-de-la-configuration)
8. [Sécurité et bonnes pratiques](#8-sécurité-et-bonnes-pratiques)
9. [Dépannage](#9-dépannage)

---

## 1. Vue d'ensemble des clés nécessaires

Le bot nécessite plusieurs clés pour fonctionner :

| Clé | Description | Obligatoire |
|-----|-------------|-------------|
| `POLY_PRIVATE_KEY` | Clé privée du wallet pour signer les transactions | Oui |
| `POLY_PROXY_WALLET` | Adresse du Safe/Proxy Wallet Polymarket | Oui |
| `POLY_BUILDER_API_KEY` | API Key du Builder Program | Oui (gasless) |
| `POLY_BUILDER_API_SECRET` | Secret du Builder Program | Oui (gasless) |
| `POLY_BUILDER_API_PASSPHRASE` | Passphrase du Builder Program | Oui (gasless) |
| `POLY_RPC_URL` | URL du noeud RPC Polygon | Optionnel |

### Architecture d'authentification Polymarket

```
┌─────────────────────────────────────────────────────────────┐
│                    POLYMARKET TRADING                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ Private Key  │───▶│ Proxy Wallet │───▶│ CLOB API     │  │
│  │ (Signer)     │    │ (Safe)       │    │ (Trading)    │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│         │                                        │          │
│         │            ┌──────────────┐           │          │
│         └───────────▶│ Builder Keys │───────────┘          │
│                      │ (Gasless)    │                      │
│                      └──────────────┘                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Créer un compte Polymarket

### Étape 2.1 : Accéder à Polymarket

1. Ouvrir https://polymarket.com dans un navigateur
2. Cliquer sur **"Sign In"** ou **"Get Started"**

### Étape 2.2 : Choisir une méthode de connexion

| Méthode | Description | Recommandation |
|---------|-------------|----------------|
| **Google** | Connexion rapide, crée un Proxy Wallet automatiquement | Débutants |
| **Email** | Magic link par email | Débutants |
| **MetaMask** | Wallet existant | Utilisateurs avancés |
| **WalletConnect** | Connexion via QR code | Mobile |

### Étape 2.3 : Compléter l'inscription

1. Accepter les conditions d'utilisation
2. Vérifier ton email si nécessaire
3. Polymarket va automatiquement créer un **Proxy Wallet** (Safe) pour toi

> **Note** : Le Proxy Wallet est un smart contract wallet qui permet le trading gasless.

---

## 3. Récupérer l'adresse Proxy Wallet

### Étape 3.1 : Accéder aux paramètres

1. Cliquer sur ton **avatar/profil** en haut à droite
2. Sélectionner **"Settings"** ou **"Paramètres"**

### Étape 3.2 : Trouver l'adresse du wallet

1. Aller dans la section **"Account"** ou **"Wallet"**
2. Chercher **"Proxy Wallet Address"** ou **"Safe Address"**
3. L'adresse ressemble à : `0x1234...abcd`

### Étape 3.3 : Copier l'adresse

```
Exemple d'adresse Proxy Wallet :
0x742d35Cc6634C0532925a3b844Bc9e7595f2bD71
```

**Copier cette adresse** → C'est ta valeur `POLY_PROXY_WALLET`

### Vérification

Tu peux vérifier ton wallet sur Polygonscan :
```
https://polygonscan.com/address/TON_PROXY_WALLET
```

---

## 4. Obtenir les Builder API Keys

Le **Builder Program** permet le trading gasless (sans frais de gas).

### Étape 4.1 : Accéder à la page API Keys

1. Aller sur https://polymarket.com/settings
2. Chercher la section **"API Keys"** ou **"Developer"**

### Étape 4.2 : Créer une nouvelle API Key

1. Cliquer sur **"Create API Key"** ou **"Generate New Key"**
2. Donner un nom descriptif (ex: "Trading Bot")
3. Confirmer la création

### Étape 4.3 : Sauvegarder les credentials

Après création, tu obtiendras **3 valeurs** :

| Credential | Variable d'environnement | Format |
|------------|-------------------------|--------|
| API Key | `POLY_BUILDER_API_KEY` | UUID (ex: `a1b2c3d4-e5f6-...`) |
| Secret | `POLY_BUILDER_API_SECRET` | Base64 string |
| Passphrase | `POLY_BUILDER_API_PASSPHRASE` | String aléatoire |

> **IMPORTANT** : Ces valeurs ne sont affichées qu'une seule fois ! Copie-les immédiatement dans un endroit sûr.

### Étape 4.4 : Tiers du Builder Program

| Tier | Transactions/jour | Comment l'obtenir |
|------|-------------------|-------------------|
| **Unverified** | 100 | Automatique à la création |
| **Verified** | 1,500 | Contacter builder@polymarket.com |
| **Partner** | Illimité | Sur invitation |

**Pour upgrader** : Envoyer un email à builder@polymarket.com avec :
- Ton adresse wallet
- Description de ton projet
- Volume de trading estimé

---

## 5. Configurer la clé privée

### Option A : Créer un nouveau wallet dédié (RECOMMANDÉ)

**Pourquoi ?** Isoler les fonds du bot pour limiter les risques.

#### Méthode 1 : Via MetaMask

1. Ouvrir MetaMask
2. Cliquer sur l'icône compte → **"Create Account"**
3. Nommer le compte "Polymarket Bot"
4. Exporter la clé privée (voir ci-dessous)

#### Méthode 2 : Via script (avancé)

```javascript
// generate-wallet.js
const { Wallet } = require('ethers');

const wallet = Wallet.createRandom();
console.log('Address:', wallet.address);
console.log('Private Key:', wallet.privateKey);
console.log('Mnemonic:', wallet.mnemonic.phrase);

// SAUVEGARDER CES INFORMATIONS EN LIEU SÛR !
```

### Option B : Utiliser un wallet existant

> **ATTENTION** : Ne jamais utiliser ton wallet principal avec des fonds importants !

### Exporter la clé privée depuis MetaMask

1. Ouvrir MetaMask
2. Cliquer sur les **3 points** à côté du compte
3. Sélectionner **"Account details"** / **"Détails du compte"**
4. Cliquer sur **"Export Private Key"** / **"Exporter la clé privée"**
5. Entrer ton mot de passe MetaMask
6. **Copier la clé privée** (commence par `0x`)

```
Exemple de clé privée :
0x4c0883a69102937d6231471b5dbb6204fe512961708279e0e2e11f44b3c4b2d8
```

> **SÉCURITÉ** : Ne jamais partager cette clé ! Quiconque la possède peut accéder à tes fonds.

---

## 6. Créer le fichier .env

### Étape 6.1 : Copier le template

```powershell
# Dans le dossier du projet
copy .env.example .env
```

### Étape 6.2 : Éditer le fichier .env

Ouvrir `.env` avec un éditeur de texte et remplir :

```env
# ============================================
# POLYMARKET TRADING BOT CONFIGURATION
# ============================================

# -----------------
# WALLET CONFIGURATION
# -----------------
# Adresse du Safe/Proxy Wallet (depuis Polymarket Settings)
POLY_PROXY_WALLET=0x_COLLER_TON_PROXY_WALLET_ICI

# Clé privée du wallet (ATTENTION: ne jamais partager !)
POLY_PRIVATE_KEY=0x_COLLER_TA_CLE_PRIVEE_ICI

# -----------------
# BUILDER PROGRAM (Gasless Trading)
# -----------------
# API Key du Builder Program
POLY_BUILDER_API_KEY=coller_ton_api_key_ici

# Secret du Builder Program (base64)
POLY_BUILDER_API_SECRET=coller_ton_secret_ici

# Passphrase du Builder Program
POLY_BUILDER_API_PASSPHRASE=coller_ton_passphrase_ici

# -----------------
# RPC CONFIGURATION
# -----------------
# URL du noeud RPC Polygon (défaut: polygon-rpc.com)
POLY_RPC_URL=https://polygon-rpc.com

# Chain ID Polygon Mainnet
POLY_CHAIN_ID=137

# -----------------
# API CONFIGURATION
# -----------------
# Host de l'API CLOB (ne pas modifier sauf si nécessaire)
POLY_CLOB_HOST=https://clob.polymarket.com

# -----------------
# APPLICATION SETTINGS
# -----------------
# Dossier pour stocker les credentials chiffrés
POLY_DATA_DIR=credentials

# Niveau de log (DEBUG, INFO, WARNING, ERROR)
POLY_LOG_LEVEL=INFO

# -----------------
# RUST LOGGING
# -----------------
RUST_LOG=info,trading_engine=debug
```

### Étape 6.3 : Vérifier le fichier .gitignore

S'assurer que `.env` est dans `.gitignore` :

```bash
# Vérifier
type .gitignore | findstr ".env"
```

Si absent, ajouter :
```
.env
.env.local
.env.*.local
```

---

## 7. Vérification de la configuration

### Test rapide des variables

```powershell
# Afficher les variables (masquées)
$env = Get-Content .env | Where-Object { $_ -notmatch '^#' -and $_ -match '=' }
foreach ($line in $env) {
    $key = $line.Split('=')[0]
    $value = $line.Split('=')[1]
    if ($value.Length -gt 10) {
        $masked = $value.Substring(0,4) + "****" + $value.Substring($value.Length-4)
    } else {
        $masked = "****"
    }
    Write-Host "$key = $masked"
}
```

### Checklist de vérification

- [ ] `POLY_PROXY_WALLET` commence par `0x` et fait 42 caractères
- [ ] `POLY_PRIVATE_KEY` commence par `0x` et fait 66 caractères
- [ ] `POLY_BUILDER_API_KEY` est un UUID valide
- [ ] `POLY_BUILDER_API_SECRET` est une chaîne base64
- [ ] `POLY_BUILDER_API_PASSPHRASE` est défini
- [ ] Le fichier `.env` est dans `.gitignore`

---

## 8. Sécurité et bonnes pratiques

### Règles fondamentales

| Règle | Description |
|-------|-------------|
| **Ne jamais commit .env** | Toujours dans .gitignore |
| **Wallet dédié** | Utiliser un wallet séparé pour le bot |
| **Fonds limités** | Ne déposer que le nécessaire pour trader |
| **Backup sécurisé** | Sauvegarder les clés dans un password manager |
| **Rotation régulière** | Changer les API keys tous les 90 jours |

### Stockage sécurisé des clés

#### Option 1 : Password Manager (recommandé)

- **Bitwarden** (gratuit, open source)
- **1Password**
- **LastPass**

#### Option 2 : Fichier chiffré

```powershell
# Chiffrer avec GPG
gpg --symmetric --cipher-algo AES256 .env

# Déchiffrer
gpg --decrypt .env.gpg > .env
```

#### Option 3 : Variables d'environnement système

```powershell
# Définir une variable permanente (PowerShell Admin)
[Environment]::SetEnvironmentVariable("POLY_PRIVATE_KEY", "0x...", "User")
```

### Ce qu'il ne faut JAMAIS faire

```
❌ Commit le fichier .env sur Git
❌ Partager les clés par email/chat
❌ Utiliser le wallet principal
❌ Stocker les clés en clair sur le cloud
❌ Réutiliser les mêmes clés sur plusieurs projets
```

---

## 9. Dépannage

### Erreur : "Invalid API Key"

**Cause** : API Key incorrecte ou expirée

**Solution** :
1. Vérifier que l'API Key est bien copiée (pas d'espaces)
2. Régénérer une nouvelle API Key sur Polymarket
3. Vérifier que le tier Builder n'est pas épuisé

### Erreur : "Invalid Signature"

**Cause** : Clé privée ne correspond pas au Proxy Wallet

**Solution** :
1. Vérifier que la clé privée correspond bien au wallet connecté à Polymarket
2. Si tu as utilisé Google/Email, tu dois exporter la clé depuis Polymarket

### Erreur : "Insufficient Funds"

**Cause** : Pas assez de USDC ou MATIC

**Solution** :
1. Déposer des USDC sur le Proxy Wallet
2. Avoir un peu de MATIC pour les transactions non-gasless

### Erreur : "Rate Limit Exceeded"

**Cause** : Trop de requêtes API

**Solution** :
1. Attendre quelques minutes
2. Demander un upgrade du tier Builder
3. Optimiser le code pour réduire les appels API

### Comment récupérer la clé privée d'un wallet Polymarket (Google/Email)

Si tu t'es connecté avec Google ou Email, Polymarket utilise un wallet MPC. Pour obtenir la clé privée :

1. Aller dans **Settings** → **Account**
2. Chercher **"Export Wallet"** ou **"Backup"**
3. Suivre les instructions de vérification
4. La clé privée sera affichée

> **Note** : Cette option peut ne pas être disponible selon le type de wallet.

---

## Résumé des étapes

```
1. Créer compte Polymarket (https://polymarket.com)
         │
         ▼
2. Copier Proxy Wallet Address (Settings → Account)
         │
         ▼
3. Créer Builder API Keys (Settings → API Keys)
         │
         ▼
4. Exporter Private Key (MetaMask ou nouveau wallet)
         │
         ▼
5. Créer fichier .env avec toutes les clés
         │
         ▼
6. Vérifier que .env est dans .gitignore
         │
         ▼
7. Tester la configuration
```

---

## Liens utiles

| Ressource | URL |
|-----------|-----|
| Polymarket | https://polymarket.com |
| Documentation API | https://docs.polymarket.com |
| Builder Program | https://docs.polymarket.com/developers/builders/builder-intro |
| Polygonscan | https://polygonscan.com |
| Support Builder | builder@polymarket.com |

---

## Contact Support

- **Builder Program** : builder@polymarket.com
- **Support général** : support@polymarket.com
- **Discord** : https://discord.gg/polymarket

---

*Document créé le 2 février 2025*
