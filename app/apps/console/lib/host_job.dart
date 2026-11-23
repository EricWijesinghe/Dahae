// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/foundation.dart';

/// How often a waiting screen asks for a server job's state. Tests shorten it.
@visibleForTesting
Duration hostJobPoll = const Duration(seconds: 2);

/// How long a job may stay `queued` before the wait gives up. The agent wakes on the job file and every minute,
/// so minutes without a pickup mean it is stopped or busy with a long job. Tests shorten it.
@visibleForTesting
Duration hostJobClaimLimit = const Duration(minutes: 3);

/// States after which a job never changes again. `unknown` means the agent never picked it up.
const finalJobStates = {
  'ok',
  'failed',
  'rolled_back',
  'rejected',
  'cancelled',
  'unknown',
};

/// Queues one server job. Owner kinds need [headers] with the owner's code.
Future<Json> queueHostJob(
  ApiClient api,
  String kind,
  String arg, [
  Map<String, String>? headers,
]) async =>
    await api.post(
          '/admin/host/jobs',
          {'kind': kind, 'arg': arg},
          ApiClient.newIdempotencyKey(),
          headers,
        )
        as Json;

/// Polls job [id] until its state is final or in [until]. After [limit], or [hostJobClaimLimit] still
/// `queued`, it returns the last answer as is.
Future<Json> waitHostJob(
  ApiClient api,
  String id, {
  Set<String> until = const {},
  Duration limit = const Duration(minutes: 10),
}) async {
  final rounds = limit.inMicroseconds ~/ hostJobPoll.inMicroseconds;
  final claimRounds =
      hostJobClaimLimit.inMicroseconds ~/ hostJobPoll.inMicroseconds;
  var job = await api.get('/admin/host/jobs/$id') as Json;
  for (var i = 0; i < rounds; i++) {
    final state = job['state'];
    if (finalJobStates.contains(state) || until.contains(state)) break;
    if (state == 'queued' && i >= claimRounds) break;
    await Future<void>.delayed(hostJobPoll);
    job = await _poll(api, id) ?? job;
  }
  return job;
}

/// One poll. A domain move or restart recreates the API, so offline and 5xx answers mean "ask again".
Future<Json?> _poll(ApiClient api, String id) async {
  try {
    return await api.get('/admin/host/jobs/$id') as Json;
  } on ApiException catch (e) {
    if (e.status == 0 || e.status >= 500) return null;
    rethrow;
  }
}

/// The job's published output (untrusted text), or null when there is none (pruned after 10 minutes).
Future<String?> hostJobOutput(ApiClient api, String id) async {
  try {
    final body = await api.get('/admin/host/jobs/$id/output');
    return body is Json ? body['text'] as String? : null;
  } on ApiException catch (e) {
    if (e.status == 404) return null;
    rethrow;
  }
}

/// The agent's last step line, the reason shown when a job is refused or rolled back. A job the agent never
/// picked up has no step: it says so.
String? jobReason(Json job, T t) {
  if (job['state'] == 'queued' || job['state'] == 'unknown') {
    return t.hjAgentSilent;
  }
  final step = job['step'];
  return step is String && step.trim().isNotEmpty ? step.trim() : null;
}
