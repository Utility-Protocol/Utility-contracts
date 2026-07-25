jest.mock('stellar-sdk', () => ({
  Server: jest.fn().mockImplementation(() => ({})),
  Networks: { PUBLIC: 'public', TESTNET: 'testnet' },
  TransactionBuilder: jest.fn(),
  Operation: {},
  Asset: {},
  Keypair: {}
}));

const ContractInterface = require('../src/contract-interface');
const CacheLayer = require('../src/cache-layer');

const contractConfig = {
  network: 'testnet',
  rpcUrl: 'https://soroban-testnet.stellar.org',
  horizonUrl: 'https://horizon-testnet.stellar.org',
  contractId: 'test-contract',
  friendbotUrl: 'https://friendbot.stellar.org',
  cache: {
    enabled: true,
    defaultTtlSeconds: 60,
    meterTtlSeconds: 60,
    usageTtlSeconds: 60,
    keyPrefix: 'test',
    redis: { enabled: false }
  }
};

describe('ContractInterface cache integration', () => {
  test('caches getMeter calls and reports metrics', async () => {
    const contract = new ContractInterface({
      ...contractConfig,
      cacheLayer: new CacheLayer(contractConfig.cache)
    });
    const spy = jest.spyOn(contract, '_simulateContractCall');

    await contract.getMeter(42);
    await contract.getMeter(42);

    expect(spy).toHaveBeenCalledTimes(1);
    expect(contract.getCacheMetrics()).toMatchObject({ hits: 1, misses: 1 });
  });

  test('invalidates meter and usage caches after usage submission', async () => {
    const contract = new ContractInterface({
      ...contractConfig,
      cacheLayer: new CacheLayer(contractConfig.cache)
    });
    await contract.getMeter(7);
    await contract.getUsageData(7);

    const usage = {
      meter_id: 7,
      timestamp: Math.floor(Date.now() / 1000),
      watt_hours_consumed: 1000,
      display_watt_hours: 1,
      units_consumed: 1,
      signature: Buffer.alloc(64).toString('base64'),
      public_key: Buffer.alloc(32).toString('base64'),
      is_peak_hour: false,
      effective_rate: 10
    };
    await contract.submitUsageData(usage);

    expect(contract.getCacheMetrics().deletes).toBe(2);
  });
});
