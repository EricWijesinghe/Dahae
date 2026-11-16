// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Push notifications: Firebase on native builds, a no-op on web (same API).
export 'push_target.dart';
export 'push_stub.dart' if (dart.library.io) 'push_io.dart';
