// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:http/http.dart' as http;

import 'api.dart';
import 'models.dart';

/// Secure-storage key of the server address found by [ApiBaseDiscovery], used from the next cold start on.
/// The value is `<compiled base>|<found base>`: a later build of the same app id with another compiled
/// address (staging, another server) ignores it.
const apiBaseOverrideKey = 'api_base_override';

/// `https://host[:port]` for a plain https server address, else null (no path, query, fragment or user info).
String? httpsBase(Object? v) {
  if (v is! String) return null;
  final u = Uri.tryParse(v);
  if (u == null ||
      u.scheme != 'https' ||
      u.host.isEmpty ||
      u.userInfo.isNotEmpty ||
      u.hasQuery ||
      u.hasFragment ||
      (u.path.isNotEmpty && u.path != '/')) {
    return null;
  }
  return u.hasPort ? 'https://${u.host}:${u.port}' : 'https://${u.host}';
}

/// The address the customer and partner apps start with: the saved move first, then the compiled one.
/// Web always uses its origin ([compiled]); the console keeps its own address store.
Future<String> resolveApiBase({
  FlutterSecureStorage storage = const FlutterSecureStorage(),
  bool web = kIsWeb,
  String Function() compiled = defaultApiBase,
}) async {
  if (!web) {
    try {
      final value = await storage.read(key: apiBaseOverrideKey) ?? '';
      final i = value.lastIndexOf('|');
      final saved = httpsBase(value.substring(i + 1));
      if (i > 0 && value.substring(0, i) == compiled() && saved != null) {
        return saved;
      }
    } on Exception catch (e) {
      debugPrint('saved server address unreadable: $e');
    }
  }
  return compiled();
}

/// Follows a domain move: after a `/v1/config` answer whose `api_base` differs from the current address,
/// asks that address for its `/v1/config` and keeps it only when it reports the same `install_id`.
/// The saved address applies at the next cold start, never mid-session (payments, sockets).
/// The address is only ever taken from `/v1/config`, never from a deep link or a push.
class ApiBaseDiscovery {
  ApiBaseDiscovery({
    required this.api,
    FlutterSecureStorage storage = const FlutterSecureStorage(),
    http.Client? client,
    this.web = kIsWeb,
    this.timeout = const Duration(seconds: 10),
    this.compiled = defaultApiBase,
  }) : _storage = storage,
       _http = client ?? http.Client();

  final ApiClient api;
  final bool web;
  final Duration timeout;

  /// The build's own address, saved with the found one (see [apiBaseOverrideKey]).
  final String Function() compiled;
  final FlutterSecureStorage _storage;
  final http.Client _http;
  String? _tried;

  /// Call with each successful `/v1/config` body from the current address. Never throws.
  Future<void> check(Json config) async {
    final next = httpsBase(config['api_base']);
    final current = httpsBase(api.baseUrl);
    final id = config['install_id'];
    if (next == null || current == null) return;
    if (next == current) {
      api.publicBase.value = next;
      return;
    }
    if (id is! String || id.isEmpty || next == _tried) return;
    _tried = next;
    if (!await _sameInstall(next, id)) return;
    api.publicBase.value = next;
    if (web) return;
    try {
      await _storage.write(
        key: apiBaseOverrideKey,
        value: '${compiled()}|$next',
      );
    } on Exception catch (e) {
      debugPrint('new server address not saved: $e');
    }
  }

  Future<bool> _sameInstall(String base, String id) async {
    try {
      final res = await _http
          .get(Uri.parse('$base/v1/config'))
          .timeout(timeout);
      if (res.statusCode != 200) return false;
      final body = jsonDecode(utf8.decode(res.bodyBytes));
      return body is Map && body['install_id'] == id;
    } on Object catch (e) {
      // Unreachable, timed out or not JSON: the current address stays.
      debugPrint('server address probe failed: $e');
      return false;
    }
  }
}
