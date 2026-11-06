// Only for the Rust mock-server integration test. Never targets a real wallet.
import assert from 'node:assert/strict';
import { connect, parse, stringify } from './client.mjs';
const cliche = connect(process.argv[2]);
const events = cliche.subscribe();
let subscriptionReady;
const ready = new Promise(resolve => { subscriptionReady = resolve; });
let eventReceived;
const received = new Promise((resolve, reject) => {
  eventReceived = resolve;
  events.on('error', reject);
});
events.on('data', ({ json }) => {
  const event = parse(json);
  if (event.kind === 'connection') subscriptionReady();
  if (event.method === 'payment_received') eventReceived(event);
});
try {
  await ready;
  const params = parse('{"msatoshi":9007199254740993}');
  const result = await cliche.call('echo', params);
  assert.equal(stringify(result.result), stringify(params));
  await cliche.call('trigger');
  const event = await received;
  assert.equal(event.params.msatoshi.toString(), '9007199254740993');
  console.log('Node gRPC command/event lossless roundtrip passed.');
} finally { events.cancel(); cliche.close(); }
