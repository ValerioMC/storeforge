# StoreForge

StoreForge is a standalone desktop client for operators who manage S3-compatible storage, including Ceph RGW. It uses the credentials and permissions of the selected connection. The app runs on macOS, Windows, and Linux with no application server.

## Audience and work

Operators move between endpoints, inspect buckets and objects, and carry out transfers and maintenance. Storage administrators also need a clear boundary between standard S3 actions and provider-specific administration.

## Working assumptions

Object browsing is the primary workspace. Connection setup, bucket configuration, administrative operations, and transfer history are accessible from it. Endpoint permissions govern shared resources. Local operation history belongs to the current desktop installation.

## Design principles

- Keep the connection, bucket, prefix, and selected object visible at the point of action.
- Separate credential setup from everyday browsing.
- Surface precise results and partial failures for changes that affect multiple objects.
- Give administrative actions a deliberate review step and distinguish remote effects from local preferences.

## Visual commitments

StoreForge uses a deep navy and cyan palette inspired by the European Space Agency, with cool-white work areas. The supplied bucket-and-anvil artwork is the product mark. Rekall informs the density and organization of the desktop workspace.
