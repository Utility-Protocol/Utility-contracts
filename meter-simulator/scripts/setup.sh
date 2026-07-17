#!/bin/bash

# Meter Simulator Setup Script
# This script sets up the meter simulator for development

set -e

echo "🚀 Setting up Meter Simulator..."

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo "❌ Node.js is not installed. Please install Node.js 16+ first."
    exit 1
fi

# Check Node.js version
NODE_VERSION=$(node -v | cut -d'v' -f2)
REQUIRED_VERSION="16.0.0"

if [ "$(printf '%s\n' "$REQUIRED_VERSION" "$NODE_VERSION" | sort -V | head -n1)" != "$REQUIRED_VERSION" ]; then
    echo "❌ Node.js version $NODE_VERSION is too old. Please upgrade to 16+."
    exit 1
fi

echo "✅ Node.js version $NODE_VERSION detected"

# Install dependencies
echo "📦 Installing dependencies..."
npm install

# Create .env file if it doesn't exist
if [ ! -f .env ]; then
    echo "📝 Creating .env file from template..."
    cp .env.example .env
    echo "⚠️  Please edit .env file with your configuration"
fi

# Make CLI executable
echo "🔧 Making CLI executable..."
chmod +x src/index.js

# Create logs directory
mkdir -p logs

# Generate test keys
echo "🔑 Generating test device keys..."
node src/index.js generate-keys --output test-device-keys.json

echo "✅ Setup complete!"
echo ""
echo "📋 Next steps:"
echo "1. Edit .env file with your configuration"
echo "2. Register a meter: node src/index.js register --keys test-device-keys.json"
echo "3. Start simulation: node src/index.js simulate --config meter-config.json"
echo ""
echo "📖 For more information, see README.md"
