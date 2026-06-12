# 🚀 IoT-Billing-Service Deployment Script

Quick and easy deployment of the IoT-Billing-Service smart contract to Stellar testnet or mainnet.

## Features

✅ **One-Command Deployment** - Deploy with a single command  
✅ **Docker-Based** - No need to install Soroban CLI locally  
✅ **Testnet & Mainnet** - Support for both networks  
✅ **Automatic Key Generation** - Creates new keypair or use existing  
✅ **Friendbot Integration** - Auto-funds testnet accounts  
✅ **Contract Building** - Automatically builds Rust contract if needed  
✅ **Verification** - Verifies deployment and provides explorer links  

---

## Quick Start

### Deploy to Testnet (Recommended for Testing)

```bash
cd scripts
chmod +x deploy.sh
./deploy.sh --network testnet
```

That's it! The script will:
1. Pull the Stellar Docker image
2. Build the contract (if needed)
3. Generate a new keypair
4. Fund the account via Friendbot
5. Deploy the contract
6. Provide you with the contract ID and explorer link

---

## Usage

### Basic Usage

```bash
# Deploy to testnet
./deploy.sh --network testnet

# Deploy to mainnet (use existing key)
./deploy.sh --network mainnet --key "SCRETKEY..."
```

### Command Options

```
Usage: ./deploy.sh --network <testnet|mainnet> [--key <secret-key>]

Options:
  --network, -n     Target network (testnet or mainnet) [REQUIRED]
  --key, -k         Secret key for deployment account (optional)
  --help, -h        Show this help message
```

### Examples

```bash
# Deploy to testnet with auto-generated key
./deploy.sh -n testnet

# Deploy to mainnet with specific key
./deploy.sh -n mainnet -k "SB2TVKWXY...YOUR_SECRET_KEY"

# View help
./deploy.sh --help
```

---

## What Gets Deployed

### Contract Details

- **Contract Name**: IoT-Billing-Service
- **Network**: Stellar (Soroban)
- **WASM Format**: WebAssembly
- **Contract Size**: ~100-200 KB

### Supported Tokens

The contract supports:
- ✅ Native XLM
- ✅ SPL tokens (SAC-compliant)
- ✅ Custom tokens

---

## Pre-Deployment Checklist

### For Testnet

- [ ] Docker installed and running
- [ ] Internet connection
- [ ] ~5 minutes for deployment
- [ ] Bash shell available

### For Mainnet

- [ ] All testnet requirements
- [ ] Sufficient XLM balance (recommended: 10+ XLM)
- [ ] Secret key for deployment account
- [ ] Double-checked network setting
- [ ] Ready to deploy real value

---

## Step-by-Step Process

### Step 1: Requirements Check

The script verifies:
- Docker is installed and running
- Rust/Cargo is available (for building)
- jq is installed (for JSON parsing)

### Step 2: Docker Image Pull

Pulls the official Stellar quickstart image:
```bash
docker pull stellar/quickstart:latest
```

### Step 3: Contract Build

If the contract hasn't been built yet:
```bash
cargo build --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/utility_contracts.wasm`

### Step 4: Container Setup

Starts a Stellar container configured for your network:
```bash
docker run -d \
  --name stellar-deploy \
  -p 8000:8000 \
  -e NETWORK=testnet \
  stellar/quickstart:latest
```

### Step 5: Keypair Setup

**Option A: Auto-Generate (Testnet)**
- Generates new Ed25519 keypair
- Funds account via Friendbot (~10,000 XLM)

**Option B: Use Existing Key (Mainnet)**
- Uses your provided secret key
- ⚠️ Ensure sufficient balance

### Step 6: Contract Deployment

Uploads the WASM file and creates the contract:
```bash
soroban contract deploy \
  --source-account <SECRET_KEY> \
  --network <NETWORK> \
  --wasm utility_contracts.wasm
```

### Step 7: Verification

Verifies deployment and provides:
- Contract ID
- Block explorer link
- Transaction hash

---

## Post-Deployment

### Access Your Contract

After successful deployment, you'll receive:

```
╔═══════════════════════════════════════════════════════════╗
║          🎉 UTILITY DRIP DEPLOYMENT COMPLETE 🎉           ║
╠═══════════════════════════════════════════════════════════╣
║  Network:          testnet                                ║
║  Contract ID:      CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS
║  Deployer Account: GABC...XYZ                             ║
║                                                           ║
║  Block Explorer:                                          ║
║  https://stellar.expert/explorer/testnet/contract/...    ║
╚═══════════════════════════════════════════════════════════╝
```

### Save Contract Information

The script creates `deployment-info.json`:

```json
{
  "contract_id": "CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS",
  "network": "testnet",
  "deployed_at": "2026-03-26T14:30:00Z",
  "deployer_account": "GABC...XYZ",
  "wasm_hash": "abc123...",
  "container_name": "stellar-deploy"
}
```

### Next Steps

1. **Register a Meter**
   ```bash
   cd ../meter-simulator
   node src/index.js register --keys device-keys.json
   ```

2. **View on Block Explorer**
   - Open the provided explorer URL
   - Verify contract code
   - Monitor transactions

3. **Interact with Contract**
   ```bash
   # Using TypeScript bindings
   npm start -- claim --meter-id 1
   
   # Or use the web interface
   ```

---

## Troubleshooting

### Issue: Docker daemon not running

**Solution:**
```bash
# macOS
open -a Docker

# Linux
sudo systemctl start docker

# Windows
Start Docker Desktop
```

---

### Issue: Contract build fails

**Solution:**
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WASM target
rustup target add wasm32-unknown-unknown

# Try building again
cd contracts/utility_contracts
cargo build --target wasm32-unknown-unknown --release
```

---

### Issue: Friendbot funding fails

**Possible causes:**
- Account already funded
- Friendbot rate limit
- Network congestion

**Solution:**
```bash
# Check account balance
curl https://horizon-testnet.stellar.org/accounts/YOUR_PUBLIC_KEY

# If already funded, proceed with deployment
# Friendbot gives 10,000 XLM per account
```

---

### Issue: Deployment transaction fails

**Check:**
1. Account has sufficient balance (≥ 1 XLM)
2. Network is correct (testnet vs mainnet)
3. Secret key is valid
4. RPC endpoint is accessible

**Retry:**
```bash
# Clean up and retry
docker stop stellar-deploy
docker rm stellar-deploy
./deploy.sh --network testnet
```

---

### Issue: Container won't start

**Solution:**
```bash
# Check if port 8000 is in use
lsof -i :8000

# Stop conflicting container
docker stop $(docker ps -q --filter "publish=8000")

# Or use different port
docker run -d -p 8001:8000 ...
```

---

## Advanced Usage

### Custom Docker Image

Use a specific Stellar version:

```bash
export DOCKER_IMAGE="stellar/quickstart:21.0"
./deploy.sh --network testnet
```

### Reuse Existing Container

Skip container creation if already running:

```bash
# Container should be named 'stellar-deploy'
docker ps | grep stellar-deploy
./deploy.sh --network testnet
```

### Manual Key Generation

Generate keys separately:

```bash
# Using Docker
docker run --rm stellar/quickstart:latest stellar-keys generate

# Output:
# Public Key: GABC...
# Secret Key: SDEF...
```

Save the secret key securely and use it in deployment:

```bash
./deploy.sh --network testnet --key "SDEF..."
```

### Batch Deployment

Deploy multiple contracts:

```bash
#!/bin/bash
for network in testnet mainnet; do
  ./deploy.sh --network $network --key "$SECRET_KEY_$network"
done
```

---

## Security Considerations

### 🔐 Secret Key Management

**Best Practices:**
1. **Never commit keys to git**
   ```bash
   echo "*.env" >> .gitignore
   echo "keys/" >> .gitignore
   ```

2. **Use environment variables**
   ```bash
   export DEPLOY_KEY="SCRET..."
   ./deploy.sh --network mainnet --key "$DEPLOY_KEY"
   ```

3. **Store keys securely**
   - Use a password manager
   - Hardware wallet for mainnet
   - Encrypted storage

4. **Rotate keys regularly**
   - Generate new keys for each deployment
   - Transfer contract ownership if needed

---

### ⚠️ Mainnet Warnings

Before deploying to mainnet:

1. **Verify contract code**
   - Audit the Rust code
   - Test thoroughly on testnet
   - Review security implications

2. **Use minimal funds**
   - Only deploy what's necessary
   - Keep majority in cold storage
   - Use multi-sig if possible

3. **Double-check network**
   - Confirm `--network mainnet`
   - Verify RPC endpoints
   - Check explorer URLs

4. **Monitor closely**
   - Set up alerts
   - Watch contract activity
   - Regular audits

---

## Container Management

### View Logs

```bash
# Follow logs in real-time
docker logs -f stellar-deploy

# Last 100 lines
docker logs --tail 100 stellar-deploy

# With timestamps
docker logs -ft stellar-deploy
```

### Stop Container

```bash
docker stop stellar-deploy
docker rm stellar-deploy
```

### Restart Container

```bash
docker start stellar-deploy
```

### Access Container Shell

```bash
docker exec -it stellar-deploy /bin/bash
```

---

## Environment Variables

Configure via environment:

```bash
export DOCKER_IMAGE="stellar/quickstart:latest"
export CONTAINER_NAME="stellar-deploy"
export NETWORK="testnet"

./deploy.sh --network $NETWORK
```

---

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Deploy Contract

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        target: wasm32-unknown-unknown
    
    - name: Deploy to Testnet
      run: |
        chmod +x scripts/deploy.sh
        ./scripts/deploy.sh --network testnet
      env:
        DEPLOY_KEY: ${{ secrets.DEPLOY_KEY }}
    
    - name: Upload Contract Info
      uses: actions/upload-artifact@v3
      with:
        name: deployment-info
        path: deployment-info.json
```

---

## Uninstall

### Remove Everything

```bash
# Stop and remove container
docker stop stellar-deploy
docker rm stellar-deploy

# Remove Docker image
docker rmi stellar/quickstart:latest

# Remove deployment files
rm deployment-info.json
rm -rf scripts/__pycache__
```

---

## Additional Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Stellar Expert Explorer](https://stellar.expert/)
- [IoT-Billing-Service Docs](../README.md)

---

## Support

Need help?

1. Check this README
2. Review troubleshooting section
3. Check container logs: `docker logs stellar-deploy`
4. Open an issue on GitHub

---

## Version History

### v1.0.0 (March 26, 2026)
- Initial release
- Testnet and mainnet support
- Automatic key generation
- Docker-based deployment
- Contract building integration
- Verification and explorer links

---

**Last Updated**: March 26, 2026  
**Version**: 1.0.0  
**Maintainer**: IoT-Billing-Service Team
