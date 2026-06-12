# IoT-Billing-Service - Usage Dashboard

A modern, real-time dashboard for visualizing kWh usage vs. XLM spend in the IoT-Billing-Service smart contract system.

## Features

### 🚀 Real-Time Monitoring
- **Live Usage Data**: Updates every 5 seconds with simulated real-time data
- **Dynamic Pricing**: Shows current rate based on peak/off-peak hours
- **Interactive Charts**: Beautiful visualizations using Recharts

### 📊 Comprehensive Analytics
- **24 Hour Overview**: Track usage patterns throughout the day
- **Cost Analysis**: Monitor XLM spending alongside energy consumption
- **Peak Hour Detection**: Visual indicators for peak pricing periods
- **Historical Trends**: View usage patterns over time

### 💡 Smart Features
- **Rate Schedule**: Clear display of peak (18:00-21:00 UTC) vs off-peak hours
- **Meter Information**: Detailed account and contract information
- **System Status**: Real-time connection and operational status
- **Responsive Design**: Works seamlessly on desktop and mobile devices

## Technology Stack

- **Next.js 14**: React framework with App Router
- **TypeScript**: Type-safe development
- **Tailwind CSS**: Modern utility-first styling
- **Recharts**: Powerful charting library
- **Lucide React**: Beautiful icon components

## Getting Started

### Prerequisites
- Node.js 16+ 
- npm or yarn

### Installation

1. Clone the repository:
```bash
git clone https://github.com/IoT-Billing-Service/Billing-contracts.git
cd Billing-contracts/usage-dashboard
```

2. Install dependencies:
```bash
npm install
```

3. Run the development server:
```bash
npm run dev
```

4. Open [http://localhost:3000](http://localhost:3000) in your browser.

## Usage

### Dashboard Components

1. **Stats Cards**: Display key metrics including 24h usage, cost, current rate, and daily averages
2. **Usage Chart**: Interactive chart showing power consumption (Wh) and cost (XLM) over time
3. **Meter Information**: Detailed account information including rates and balance
4. **Rate Schedule**: Visual representation of peak and off-peak hours
5. **System Status**: Real-time connection and operational status indicators

### Real-Time Updates

The dashboard automatically updates every 5 seconds when in "Live" mode. You can pause real-time updates using the toggle in the header.

### Peak Hour Detection

- **Peak Hours**: 18:00 - 21:00 UTC (1.5x rate multiplier)
- **Off-Peak Hours**: All other times (base rate)
- Visual indicators show current pricing period

## Data Model

### UsageData
```typescript
interface UsageData {
  timestamp: string;
  kWh: number;
  XLM: number;
  rate: number;
  isPeakHour: boolean;
}
```

### MeterData
```typescript
interface MeterData {
  id: string;
  user: string;
  provider: string;
  offPeakRate: number;
  peakRate: number;
  balance: number;
  totalUsage: number;
  totalSpend: number;
  lastUpdate: string;
}
```

## Integration with Smart Contracts

This dashboard is designed to work with the IoT-Billing-Service smart contracts:

- **Contract ID**: CB7PSJZALNWNX7NLOAM6LOEL4OJZMFPQZJMIYO522ZSACYWXTZIDEDSS
- **Network**: Stellar Testnet
- **Rate Structure**: Variable rate tariffs with peak hour multipliers

## Development

### Project Structure
```
usage-dashboard/
├── src/
│   ├── app/                 # Next.js App Router
│   ├── components/          # React components
│   ├── lib/                # Utility functions and mock data
│   └── types/              # TypeScript type definitions
├── public/                 # Static assets
└── README.md
```

### Available Scripts

- `npm run dev` - Start development server
- `npm run build` - Build for production
- `npm run start` - Start production server
- `npm run lint` - Run ESLint

## Future Enhancements

- [ ] Connect to real Stellar blockchain data
- [ ] Add user authentication and wallet integration
- [ ] Implement historical data persistence
- [ ] Add export functionality for reports
- [ ] Mobile app version
- [ ] Integration with hardware meters

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

For support and questions:
- Create an issue in the GitHub repository
- Join our community discussions
- Check the [documentation](../README.md)
