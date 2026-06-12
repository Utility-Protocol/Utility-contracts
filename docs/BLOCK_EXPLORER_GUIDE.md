# 🔍 Verifying Usage Drips on Stellar Block Explorer

This guide shows users how to verify their utility consumption data ("Usage Drips") directly on the Stellar block explorer. Every transaction and event is publicly verifiable on-chain.

## Overview

The IoT-Billing-Service smart contract records all usage data on the Stellar blockchain, providing:
- ✅ **Transparency** - All consumption data is publicly verifiable
- ✅ **Immutability** - Data cannot be altered once recorded
- ✅ **Audit Trail** - Complete history of all transactions
- ✅ **Real-time Tracking** - Monitor usage as it happens

## Supported Block Explorers

You can use any of these explorers to view your Usage Drips:

1. **Stellar Expert** - https://stellar.expert/
2. **Stellar Chain** - https://stellarchain.io/
3. **Lumenscan** - https://lumenscan.io/
4. **Stellar.org Dashboard** - https://dashboard.stellar.org/

## Quick Start

### What You Need

Before you begin, gather this information:

1. **Contract Address**: `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS` (Testnet)
2. **Your Meter ID**: The unique identifier for your meter (e.g., `1`, `2`, `3`)
3. **Your Account Address**: Your Stellar public key (starts with `G...`)

---

## Step-by-Step Verification Guide

### Method 1: Search by Contract Address (Recommended)

#### Step 1: Navigate to Block Explorer

Open your preferred Stellar block explorer:
```
https://stellar.expert/explorer/testnet/contract/CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS
```

Replace `testnet` with `public` for mainnet deployments.

#### Step 2: View Contract Details

You'll see:
- 📊 Contract overview
- 💰 Recent transactions
- 📝 Event logs
- 👥 Contract holders

#### Step 3: Filter Transactions

Look for these transaction types:
- `deduct_units` - Usage data submissions
- `top_up` - Balance top-ups
- `claim` - Provider earnings claims
- `update_usage` - Manual usage updates

#### Step 4: Examine Transaction Details

Click on any transaction to see:
- **Transaction Hash**: Unique identifier
- **Timestamp**: When it occurred
- **From**: Who submitted it
- **Operations**: Contract method calls
- **Events**: Emitted data
- **Status**: Success/failure

---

### Method 2: Search by Meter ID

#### Step 1: Find Your Meter's Transactions

Most explorers allow searching by metadata. Use your Meter ID in the search:
```
Meter ID: 1
```

#### Step 2: Look for UsageReported Events

The contract emits events for each usage submission:
```
Event: UsageReported
├─ meter_id: 1
├─ units_consumed: 250
└─ cost: 2500 tokens
```

#### Step 3: Verify Consumption Data

Click on the event to see:
- Watt-hours consumed
- Units consumed
- Cost charged
- Timestamp of reading

---

### Method 3: Search by Your Account

#### Step 1: Search Your Address

Enter your Stellar address in the explorer:
```
GD5DJQD7Y6KQLZBXNRCRJAY5PZQIIVMV5MW4FPX3BVUBQD2ZMJ7LFQXL
```

#### Step 2: View Transaction History

You'll see all transactions involving your account:
- Meter registrations
- Top-ups
- Usage submissions
- Withdrawals

#### Step 3: Filter by Contract

Filter transactions to show only those interacting with the IoT-Billing-Service contract.

---

## Understanding Contract Events

The IoT-Billing-Service contract emits several event types that you can track:

### 1. UsageReported Event

Emitted when usage data is submitted via `deduct_units`.

**Event Data:**
```json
{
  "event_type": "UsageReported",
  "meter_id": 1,
  "units_consumed": "250",
  "cost": "2500"
}
```

**What it means:**
- `meter_id`: Which meter reported this usage
- `units_consumed`: Energy units consumed (kWh)
- `cost`: Token cost for this usage

**How to find it:**
1. Go to contract page
2. Click "Events" tab
3. Filter by "UsageReported"
4. Click to see details

---

### 2. TokenUp Event

Emitted when a user tops up their meter balance.

**Event Data:**
```json
{
  "event_type": "TokenUp",
  "meter_id": 1,
  "xlm_amount": "10000000",
  "usd_cents": "250000"
}
```

**What it means:**
- `xlm_amount`: XLM tokens added (in stroops)
- `usd_cents`: USD equivalent value

---

### 3. USDtoXLM Event

Emitted when withdrawing earnings with XLM conversion.

**Event Data:**
```json
{
  "event_type": "USDtoXLM",
  "meter_id": 1,
  "usd_cents": "5000",
  "xlm_amount": "20000000"
}
```

---

### 4. Active/Inactive Events

Emitted when meter status changes.

**Active Event:**
```json
{
  "event_type": "Active",
  "meter_id": 1,
  "timestamp": "1710000000"
}
```

**Inactive Event:**
```json
{
  "event_type": "Inactive",
  "meter_id": 1,
  "timestamp": "1710003600"
}
```

---

## Practical Examples

### Example 1: Verify Your Last Top-Up

**Scenario**: You topped up your meter and want to confirm it was processed.

**Steps:**
1. Open Stellar Expert: https://stellar.expert/
2. Paste contract address: `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS`
3. Click "Transactions" tab
4. Look for recent `top_up` operations
5. Click on the transaction
6. Verify:
   - ✅ Amount matches what you sent
   - ✅ Meter ID is correct
   - ✅ Status is "Success"
   - ✅ TokenUp event was emitted

---

### Example 2: Track Daily Consumption

**Scenario**: You want to see how much energy your meter consumed today.

**Steps:**
1. Go to contract page
2. Click "Events" tab
3. Filter by "UsageReported"
4. Look at events from today's date
5. Sum up all `units_consumed` values
6. Convert to kWh if needed (divide by precision factor)

**Example Output:**
```
Time        | Units | Cost (tokens)
------------|-------|---------------
08:00:00    | 100   | 1000
12:00:00    | 250   | 2500
18:00:00    | 150   | 2250 (peak hour!)
20:00:00    | 200   | 3000 (peak hour!)
------------|-------|---------------
Total       | 700   | 8750 tokens
```

---

### Example 3: Verify Peak Hour Pricing

**Scenario**: You want to confirm that peak hour pricing (1.5x) was applied correctly.

**Steps:**
1. Find UsageReported events during peak hours (18:00-21:00 UTC)
2. Compare cost per unit with off-peak events
3. Peak hour rate should be 1.5x higher

**Verification:**
```
Off-peak example:
- units_consumed: 100
- cost: 1000 tokens
- rate: 10 tokens/unit ✓

Peak hour example:
- units_consumed: 100
- cost: 1500 tokens
- rate: 15 tokens/unit ✓ (1.5x multiplier applied)
```

---

### Example 4: Audit Provider Withdrawals

**Scenario**: You're a provider and want to verify your withdrawal history.

**Steps:**
1. Search your provider address
2. Filter transactions to IoT-Billing-Service contract
3. Look for `withdraw_earnings` operations
4. Check amounts and timestamps
5. Verify against your records

---

## Reading Transaction Details

### Transaction Structure

When you click on a transaction, you'll see:

```
Transaction Hash: abc123...
Status: SUCCESS
Created At: 2026-03-26 14:30:00 UTC

Source Account: GD5DJQ...
Fee Paid: 100 stroops

Operations:
  └─ Invoke Host Function
      ├─ Contract ID: CB7PSJ...
      ├─ Function: deduct_units
      └─ Parameters:
          ├─ meter_id: 1
          ├─ watt_hours_consumed: 250
          └─ units_consumed: 1

Events:
  └─ UsageReported
      ├─ meter_id: 1
      ├─ units_consumed: 1
      └─ cost: 2500
```

### Understanding Parameters

**For `deduct_units`:**
- `meter_id`: Your meter identifier
- `watt_hours_consumed`: Energy consumed since last reading
- `units_consumed`: Converted units (typically kWh)
- `signature`: Device signature (cryptographic proof)
- `public_key`: Device public key

**For `top_up`:**
- `meter_id`: Target meter
- `amount`: Tokens to add

---

## Advanced Queries

### Export Your Data

Most explorers allow exporting transaction history:

1. **CSV Export**: Download as spreadsheet
2. **JSON Export**: Machine-readable format
3. **API Access**: Programmatic queries

**Example API Query (Stellar Expert):**
```bash
curl "https://api.stellar.expert/explorer/testnet/contract/CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS/events?cursor=12345&limit=100"
```

### Filter by Date Range

Use explorer's date picker to filter transactions:
- Today
- Last 7 days
- Last 30 days
- Custom range

### Monitor Multiple Meters

If you manage multiple meters:
1. Create a list of your Meter IDs
2. Search each one periodically
3. Or use explorer's watchlist feature
4. Set up alerts (if supported)

---

## Troubleshooting

### "Transaction Not Found"

**Possible causes:**
- Transaction still pending (wait ~5 seconds)
- Wrong network (testnet vs mainnet)
- Incorrect contract address
- Transaction failed

**Solution:**
1. Verify contract address
2. Check network (testnet/public)
3. Wait a few seconds and refresh
4. Search by your account instead

---

### "No Events Showing"

**Possible causes:**
- No usage data submitted yet
- Wrong filter applied
- Looking at wrong meter ID

**Solution:**
1. Clear all filters
2. Verify meter ID is correct
3. Submit a test transaction
4. Check "All Events" not just specific type

---

### "Can't Read Event Data"

Some explorers show raw XDR data. To decode:

1. Copy the event XDR
2. Use Stellar Laboratory: https://laboratory.stellar.org/
3. Paste XDR in decoder
4. View structured data

---

## Tips & Best Practices

### 🔍 Bookmark Your Contract

Save direct links for quick access:
```
Testnet: https://stellar.expert/explorer/testnet/contract/CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS
Mainnet: https://stellar.expert/explorer/public/contract/YOUR_CONTRACT_ID
```

### 📱 Set Up Alerts

Some explorers offer notification features:
- New transaction alerts
- Large top-up notifications
- Meter status change alerts

### 📊 Regular Audits

Recommended audit schedule:
- **Daily**: Check active meters
- **Weekly**: Review consumption patterns
- **Monthly**: Full reconciliation
- **Quarterly**: Complete audit trail review

### 🔐 Verify Signatures

For maximum security:
1. Note the signature in each UsageReported event
2. Verify it matches your device's public key
3. Ensure timestamp is recent (< 5 minutes)
4. Report any suspicious activity

---

## Integration with Tools

### Spreadsheet Tracking

Create a Google Sheet or Excel file to track:

| Date | Time | Meter ID | Units | Cost | TX Hash | Notes |
|------|------|----------|-------|------|---------|-------|
| Mar 26 | 08:00 | 1 | 100 | 1000 | abc123... | Normal usage |
| Mar 26 | 18:00 | 1 | 150 | 2250 | def456... | Peak hour |

### Monitoring Dashboards

Build a dashboard using:
- Explorer APIs
- Contract read methods
- Event streaming

Example tools:
- Grafana
- Tableau
- Power BI
- Custom web app

---

## Network Information

### Testnet

- **Contract**: `CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS`
- **Explorer**: https://stellar.expert/explorer/testnet/
- **RPC**: https://soroban-testnet.stellar.org/
- **Horizon**: https://horizon-testnet.stellar.org/

### Mainnet (Production)

- **Contract**: Deploy your own
- **Explorer**: https://stellar.expert/explorer/public/
- **RPC**: https://soroban-rpc.stellar.org/
- **Horizon**: https://horizon.stellar.org/

---

## FAQ

### Q: How long does it take for transactions to appear?

**A:** Typically 5-10 seconds after submission. If it takes longer, the transaction may have failed.

### Q: Can I see historical data from months ago?

**A:** Yes! All data is permanently stored on the blockchain. Use the explorer's date range filter.

### Q: Are there fees for viewing data?

**A:** No, viewing blockchain data is free. You only pay fees for submitting transactions.

### Q: How do I know which Meter ID is mine?

**A:** Meter IDs are assigned sequentially during registration. Check your registration transaction to find your Meter ID.

### Q: Can I export all my data?

**A:** Yes, most explorers support CSV/JSON export. You can also query the Horizon API directly.

---

## Additional Resources

- [Stellar Expert Documentation](https://stellar.expert/help)
- [Stellar Developer Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [IoT-Billing-Service Contract Documentation](../README.md)

---

## Support

Need help verifying your Usage Drips?

1. Check this guide first
2. Review explorer documentation
3. Contact support with:
   - Your Meter ID
   - Transaction hash in question
   - Screenshot of the issue

---

**Last Updated**: March 26, 2026  
**Contract Version**: 1.0.0  
**Network**: Testnet (Mainnet deployment available)
