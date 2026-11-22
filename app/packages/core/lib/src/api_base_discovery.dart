// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:http/http.dart' as http;

import 'api.dart';
import 'models.dart';

/// Secure-storage key of the server address found by [ApiBaseDiscovery], used from the next cold start on.
/// The value is `<compiled base>|<found base>|<install_id>[|<first miss, ms since epoch>]`: a later build of
/// the same app id with another compiled address (staging, another server) ignores it. Values saved before
/// the install_id was added (two fields) are followed but never dropped.
const apiBaseOverrideKey = 'api_base_override';

/// A saved address is dropped only after missing on cold starts at least this far apart, so a squatter on
/// the build's old address cannot take over moved installs with one short outage of the new host.
const _revertAfter = Duration(hours: 24);

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
/// A saved address that stops answering for this install while the compiled one answers with the same
/// `install_id` is dropped (a move undone, the new host gone), but only after misses on cold starts
/// [_revertAfter] apart; when neither answers (offline) the saved one stays.
/// Web always uses its origin ([compiled]); the console keeps its own address store.
Future<String> resolveApiBase({
  FlutterSecureStorage storage = const FlutterSecureStorage(),
  bool web = kIsWeb,
  String Function() compiled = defaultApiBase,
  http.Client? client,
  Duration timeout = const Duration(seconds: 4),
  DateTime Function() now = DateTime.now,
}) async {
  if (web) return compiled();
  var parts = const <String>[];
  try {
    parts = (await storage.read(key: apiBaseOverrideKey) ?? '').split('|');
  } on Exception catch (e) {
    debugPrint('saved server address unreadable: $e');
  }
  final saved = parts.length > 1 && parts[0] == compiled()
      ? httpsBase(parts[1])
      : null;
  if (saved == null || saved == compiled()) return compiled();
  final id = parts.length > 2 && parts[2].isNotEmpty ? parts[2] : null;
  if (id == null) return saved;
  final c = client ?? http.Client();
  final String? savedId;
  final String? compiledId;
  try {
    // Started together: a network that drops traffic delays the first frame by one [timeout], not two.
    final fallback = _installId(c, compiled(), timeout);
    savedId = await _installId(c, saved, timeout);
    compiledId = await fallback;
  } finally {
    if (client == null) c.close();
  }
  final keep = '${compiled()}|$saved|$id';
  final missed = parts.length > 3 ? int.tryParse(parts[3]) : null;
  final t = now().millisecondsSinceEpoch;
  if (savedId == id) {
    if (parts.length > 3) await _write(storage, keep);
  } else if (compiledId == id) {
    if (missed == null) {
      await _write(storage, '$keep|$t');
    } else if (t - missed >= _revertAfter.inMilliseconds) {
      await _write(storage, null);
      return compiled();
    }
  }
  return saved;
}

/// Writes (or with null deletes) the saved address. Never throws.
Future<void> _write(FlutterSecureStorage storage, String? value) async {
  try {
    await storage.write(key: apiBaseOverrideKey, value: value);
  } on Exception catch (e) {
    debugPrint('saved server address not updated: $e');
  }
}

/// The non-empty `install_id` of a 200 JSON `GET <base>/v1/config` within [timeout], else null: a parked
/// or catch-all page answers 200 too.
Future<String?> _installId(
  http.Client client,
  String base,
  Duration timeout,
) async {
  try {
    final res = await client.get(Uri.parse('$base/v1/config')).timeout(timeout);
    if (res.statusCode != 200) return null;
    final body = jsonDecode(utf8.decode(res.bodyBytes));
    final id = body is Map ? body['install_id'] : null;
    return id is String && id.isNotEmpty ? id : null;
  } on Object catch (e) {
    // Unreachable, timed out or not JSON.
    debugPrint('server $base not answering: $e');
    return null;
  }
}

/// Follows a domain move: after a `/v1/config` answer whose `api_base` differs from the current address,
/// asks that address for its `/v1/config` and keeps it only when it reports the same `install_id`. First the
/// current server's Caddy must name that host as the domain root approved (`/.well-known/dahae-domain`, from the
/// root-written DOMAIN): `api_base` and `install_id` both come from the API, which alone proves nothing.
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
    if (!await _rootApproved(current, next)) return;
    if (await _installId(_http, next, timeout) != id) return;
    api.publicBase.value = next;
    if (web) return;
    // A move back to the build's own address needs no saved value.
    await _write(
      _storage,
      next == compiled() ? null : '${compiled()}|$next|$id',
    );
  }

  Future<bool> _rootApproved(String current, String next) async {
    try {
      final res = await _http
          .get(Uri.parse('$current/.well-known/dahae-domain'))
          .timeout(timeout);
      return res.statusCode == 200 &&
          utf8.decode(res.bodyBytes).trim() == Uri.parse(next).host;
    } on Object catch (e) {
      debugPrint('approved domain unreadable: $e');
      return false;
    }
  }
}
