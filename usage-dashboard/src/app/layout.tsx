import './globals.css'
import type { Metadata } from 'next'

export const metadata: Metadata = {
  title: 'Utility-Protocol - Usage Dashboard',
  description: 'Real-time kWh usage vs. XLM spend visualization dashboard',
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
