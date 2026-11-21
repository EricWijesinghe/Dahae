// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'api.dart';
import 'push_target.dart';

/// Web has no push: same surface as push_io.dart, every call does nothing.
class PushService {
  PushService._();

  static Stream<PushTarget> get taps => const Stream.empty();

  static PushTarget? takePending() => null;

  static Future<void> init() async {}

  static Future<void> start(ApiClient api) async {}

  static Future<void> setChannelName(String name) async {}
}
