import { connect, parse, stringify } from './client.mjs';
const cliche = connect(process.argv[2]);
const events = cliche.subscribe();
events.on('data', ({ json }) => {
  const delivery = parse(json);
  if (delivery.method === 'payment_received') {
    // All JSON numbers use lossless representations; no implicit Number coercion.
    console.log(stringify({ payment_hash: delivery.params.payment_hash, msatoshi: delivery.params.msatoshi }));
  } else if (delivery.kind === 'gap') {
    console.error('Event gap: reconcile your persisted payment hashes.');
  }
});
events.on('error', error => { console.error(`Event stream ended (${error.code}).`); cliche.close(); });
console.log(stringify(await cliche.call('ping')));
process.on('SIGINT', () => { events.cancel(); cliche.close(); });
process.on('SIGTERM', () => { events.cancel(); cliche.close(); });
