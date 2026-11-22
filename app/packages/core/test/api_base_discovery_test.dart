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

  @override
  Future<void> delete({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async => data.remove(key);
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
      expect(s.storage.data[apiBaseOverrideKey], '$_old|$_new|$_id');
      expect(s.api.baseUrl, _old, reason: 'never switched mid-session');
      expect(s.api.publicBase.value, _new);
      expect(
        await resolveApiBase(
          storage: s.storage,
          web: false,
          compiled: () => _old,
          client: _probe(),
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

  test(
    'a move back to the build\'s own address clears the saved one',
    () async {
      final s = _setup(
        probe: _probe(approved: 'old.test'),
        base: _new,
      );
      s.storage.data[apiBaseOverrideKey] = '$_old|$_new|$_id';

      await s.discovery.check(_config(_old));

      expect(s.storage.data, isEmpty);
      expect(s.api.publicBase.value, _old);
    },
  );

  group('a saved address that stops answering', () {
    const saved = '$_old|$_new|$_id';
    const day = Duration(hours: 24);
    final t0 = DateTime.utc(2026, 11, 23, 9);

    /// `/v1/config` answers [body] (default: this install's id) on the hosts in [up]; [hang] hosts never
    /// answer, others fail.
    http.Client hosts({
      Set<String> up = const {},
      Set<String> hang = const {},
      Map<String, String> body = const {},
      List<Uri>? seen,
    }) => MockClient((req) async {
      seen?.add(req.url);
      if (hang.contains(req.url.host)) await Completer<void>().future;
      if (!up.contains(req.url.host)) throw http.ClientException('down');
      return http.Response(
        body[req.url.host] ?? jsonEncode({'install_id': _id}),
        200,
      );
    });

    Future<String> resolve(
      http.Client client,
      _MemStorage storage, {
      DateTime? at,
      Duration timeout = const Duration(milliseconds: 50),
    }) => resolveApiBase(
      storage: storage,
      web: false,
      compiled: () => _old,
      client: client,
      timeout: timeout,
      now: () => at ?? t0,
    );

    String missAt(DateTime t) => '$saved|${t.millisecondsSinceEpoch}';

    test('is kept while it answers with this install id', () async {
      final seen = <Uri>[];
      final storage = _MemStorage({apiBaseOverrideKey: saved});

      expect(
        await resolve(hosts(up: {'new.test', 'old.test'}, seen: seen), storage),
        _new,
      );
      expect(storage.data[apiBaseOverrideKey], saved);
      expect(seen.map((u) => '$u'), contains('$_new/v1/config'));
    });

    test(
      'falls back only after misses on cold starts a day apart, then is cleared',
      () async {
        for (final client in [
          hosts(up: {'old.test'}),
          hosts(up: {'old.test'}, hang: {'new.test'}),
          // A lapsed domain's parking page, or another install, answers 200 too.
          hosts(
            up: {'old.test', 'new.test'},
            body: {'new.test': '<html>parked</html>'},
          ),
          hosts(
            up: {'old.test', 'new.test'},
            body: {
              'new.test': jsonEncode({'install_id': 'ffffffffffffffff'}),
            },
          ),
        ]) {
          final storage = _MemStorage({apiBaseOverrideKey: saved});

          expect(await resolve(client, storage), _new, reason: 'first miss');
          expect(storage.data[apiBaseOverrideKey], missAt(t0));
          expect(await resolve(client, storage, at: t0.add(day ~/ 2)), _new);
          expect(storage.data[apiBaseOverrideKey], missAt(t0));
          expect(await resolve(client, storage, at: t0.add(day)), _old);
          expect(storage.data, isEmpty);
        }
      },
    );

    test('a miss is forgotten once the saved address answers again', () async {
      final storage = _MemStorage({apiBaseOverrideKey: missAt(t0)});

      expect(
        await resolve(
          hosts(up: {'new.test', 'old.test'}),
          storage,
          at: t0.add(day),
        ),
        _new,
      );
      expect(storage.data[apiBaseOverrideKey], saved);
    });

    test('is kept when the build\'s own address is not this install', () async {
      for (final client in [
        hosts(),
        hosts(hang: {'new.test', 'old.test'}),
        hosts(up: {'old.test'}, body: {'old.test': '<html>parked</html>'}),
        hosts(
          up: {'old.test'},
          body: {
            'old.test': jsonEncode({'install_id': 'ffffffffffffffff'}),
          },
        ),
        hosts(
          up: {'old.test'},
          body: {
            'old.test': jsonEncode({'install_id': ''}),
          },
        ),
      ]) {
        final storage = _MemStorage({apiBaseOverrideKey: missAt(t0)});

        expect(await resolve(client, storage, at: t0.add(day * 2)), _new);
        expect(storage.data[apiBaseOverrideKey], missAt(t0));
      }
    });

    test('both probes share one deadline', () async {
      final watch = Stopwatch()..start();
      await resolve(
        hosts(hang: {'new.test', 'old.test'}),
        _MemStorage({apiBaseOverrideKey: saved}),
        timeout: const Duration(milliseconds: 300),
      );
      expect(watch.elapsedMilliseconds, lessThan(550));
    });

    test(
      'a value saved without an install id, or naming the build\'s own address, is not probed',
      () async {
        for (final (value, want) in [
          ('$_old|$_new', _new),
          ('$_old|$_old|$_id', _old),
        ]) {
          final seen = <Uri>[];
          final storage = _MemStorage({apiBaseOverrideKey: value});

          expect(
            await resolve(hosts(up: {'old.test'}, seen: seen), storage),
            want,
          );
          expect(seen, isEmpty);
          expect(storage.data[apiBaseOverrideKey], value);
        }
      },
    );
  });
}
