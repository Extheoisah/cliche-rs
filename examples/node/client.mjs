import grpc from '@grpc/grpc-js';
import protoLoader from '@grpc/proto-loader';
import { fileURLToPath } from 'node:url';
import { parse, stringify } from 'lossless-json';
export { parse, stringify };
const protoPath = fileURLToPath(new URL('../../crates/cliche-gateway/proto/cliche/v1/cliche.proto', import.meta.url));
const definition = protoLoader.loadSync(protoPath, { keepCase: true, defaults: true });
const { cliche } = grpc.loadPackageDefinition(definition);

// Loopback example. Remote deployments should provide TLS credentials and a token.
export function connect(address = '127.0.0.1:12001', credentials = grpc.credentials.createInsecure()) {
  const client = new cliche.v1.ClicheService(address, credentials);
  const metadata = new grpc.Metadata();
  if (process.env.CLICHE_GATEWAY_TOKEN) metadata.set('authorization', `Bearer ${process.env.CLICHE_GATEWAY_TOKEN}`);
  return {
    client,
    call(method, params = {}, request_id = '') {
      return new Promise((resolve, reject) => {
        client.Call({ method, params_json: stringify(params), request_id }, metadata, { deadline: Date.now() + 30_000 }, (error, response) => {
          if (error) {
            const details = error.metadata?.get('grpc-status-details-bin')?.[0];
            if (Buffer.isBuffer(details)) {
              try { error.cliche = parse(details.toString('utf8')); } catch { /* may be a proxy-generated error */ }
            }
            reject(error);
          } else resolve(parse(response.json));
        });
      });
    },
    subscribe(methods = []) { return client.SubscribeEvents({ methods }, metadata); },
    close() { client.close(); }
  };
}
