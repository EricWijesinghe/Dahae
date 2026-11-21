// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:async';
import 'dart:convert';

import 'package:core/core.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

/// Minimal in-memory keystore for this test only.
class _MemStorage extends FlutterSecureStorage {
  _MemStorage([Map<String, String>? initial]) : data = {...?initial};
  final Map<String, String> data;

  @override
  Future<String?> read({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async => data[key];

  @override
  Future<void> write({
    required String key,
    required String? value,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    if (value == null) {
      data.remove(key);
    } else {
      data[key] = value;
    }
  }
}

const _old = 'https://old.test';
const _new = 'https://new.test';
const _id = '0011223344556677';

/// `/v1/config` on the probed host answers with [installId]; [hang] never answers. The current host's
/// `/.well-known/dahae-domain` (written by Caddy from the root-owned DOMAIN) answers [approved].
http.Client _probe({
  String? installId = _id,
  bool hang = false,
  List<Uri>? seen,
  String approved = 'new.test',
}) => MockClient((req) async {
  seen?.add(req.url);
  if (req.url.path == '/.well-known/dahae-domain') {
    return http.Response('$approved\n', 200);
  }
  if (hang) await Completer<void>().future;
  return http.Response(
    jsonEncode({'install_id': installId}),
    200,
    headers: {'content-type': 'application/json'},
  );
});

({ApiBaseDiscovery discovery, _MemStorage storage, ApiClient api}) _setup({
  required http.Client probe,
  String base = _old,
  bool web = false,
}) {
  final storage = _MemStorage();
  final api = ApiClient(
    baseUrl: base,
    session: Session(namespace: 'discovery-test', storage: _MemStorage()),
    client: MockClient((_) async => http.Response('{}', 200)),
  );
  return (
    discovery: ApiBaseDiscovery(
      api: api,
      storage: storage,
      client: probe,
      web: web,
      timeout: const Duration(milliseconds: 50),
      compiled: () => _old,
    ),
    storage: storage,
    api: api,
  );
}

Map<String, Object?> _config(Object? apiBase, [Object? id = _id]) => {
  'api_base': apiBase,
  'install_id': id,
};

void main() {
  test(
    'a new https base with the same install id is saved for the next start',
    () async {
      final seen = <Uri>[];
      final s = _setup(probe: _probe(seen: seen));

      await s.discovery.check(_config(_new));

      expect(seen.map((u) => '$u'), [
        '$_old/.well-known/dahae-domain',
        '$_new/v1/config',
      ]);
      expect(s.storage.data[apiBaseOverrideKey], '$_old|$_new');
      expect(s.api.baseUrl, _old, reason: 'never switched mid-session');
      expect(s.api.publicBase.value, _new);
      expect(
        await resolveApiBase(
          storage: s.storage,
          web: false,
          compiled: () => _old,
        ),
        _new,
      );
    },
  );

  test(
    'a different install id, a timeout or a failed probe changes nothing',
    () async {
      for (final probe in [
        _probe(installId: 'ffffffffffffffff'),
        _probe(installId: null),
        _probe(hang: true),
        MockClient((_) async => throw http.ClientException('down')),
        MockClient((_) async => http.Response('<html>', 502)),
      ]) {
        final s = _setup(probe: probe);

        await s.discovery.check(_config(_new));

        expect(s.storage.data, isEmpty);
        expect(s.api.publicBase.value, isNull);
      }
    },
  );

  test('only https bases with a plain host are probed', () async {
    for (final bad in [
      'http://new.test',
      'https://',
      'https://user@new.test',
      'https://new.test/path',
      'https://new.test?x=1',
      'ftp://new.test',
      'new.test',
      42,
      null,
    ]) {
      final seen = <Uri>[];
      final s = _setup(probe: _probe(seen: seen));

      await s.discovery.check(_config(bad));

      expect(seen, isEmpty, reason: '$bad');
      expect(s.storage.data, isEmpty, reason: '$bad');
    }
  });

  test(
    'the same base, a missing install id or a dev base is not probed',
    () async {
      for (final (base, config) in [
        (_old, _config(_old)),
        (_old, _config('$_old/')),
        (_old, _config(_new, null)),
        (_old, _config(_new, '')),
        ('http://127.0.0.1:8080', _config(_new)),
      ]) {
        final seen = <Uri>[];
        final s = _setup(
          probe: _probe(seen: seen),
          base: base,
        );

        await s.discovery.check(config);

        expect(seen, isEmpty, reason: '$base $config');
        expect(s.storage.data, isEmpty);
      }
    },
  );

  test('a base is probed once per run, not on every config poll', () async {
    final seen = <Uri>[];
    final s = _setup(
      probe: _probe(installId: 'ffffffffffffffff', seen: seen),
    );

    await s.discovery.check(_config(_new));
    await s.discovery.check(_config(_new));

    expect(seen, hasLength(2));
  });

  test(
    'a base the server root did not approve is never probed or saved',
    () async {
      for (final probe in [
        _probe(approved: 'evil.test'),
        _probe(approved: ''),
        MockClient(
          (req) async => req.url.path == '/.well-known/dahae-domain'
              ? http.Response('not found', 404)
              : http.Response(jsonEncode({'install_id': _id}), 200),
        ),
      ]) {
        final s = _setup(probe: probe);

        await s.discovery.check(_config(_new));

        expect(s.storage.data, isEmpty);
        expect(s.api.publicBase.value, isNull);
      }
    },
  );

  test('web never saves an override and never reads one', () async {
    final s = _setup(probe: _probe(), web: true);

    await s.discovery.check(_config(_new));

    expect(s.storage.data, isEmpty);
    final saved = _MemStorage({apiBaseOverrideKey: '$_old|$_new'});
    expect(
      await resolveApiBase(storage: saved, web: true, compiled: () => _old),
      _old,
    );
  });

  test(
    'without a saved override, or with a bad one, the compiled base is used',
    () async {
      for (final saved in [
        <String, String>{},
        {apiBaseOverrideKey: '$_old|http://evil.test'},
        // Saved by a build with another compiled address (staging, another server) on this device.
        {apiBaseOverrideKey: 'https://staging.test|$_new'},
        {apiBaseOverrideKey: _new},
      ]) {
        expect(
          await resolveApiBase(
            storage: _MemStorage(saved),
            web: false,
            compiled: () => _old,
          ),
          _old,
        );
      }
    },
  );
}
