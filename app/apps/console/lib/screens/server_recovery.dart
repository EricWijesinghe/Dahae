// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:convert';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../download.dart';
import '../host_job.dart';
import '../state.dart';
import '../widgets/owner_totp_dialog.dart';
import 'confirm_action.dart';

/// Where the sealed key copy is saved. Tests replace it so nothing lands in a real Downloads folder.
@visibleForTesting
Future<bool> Function(List<int> bytes, String filename) saveKeyCopy = saveCsv;

/// Minimum passphrase length, as `dahae keys-backup` asks.
const _minPass = 12;

/// The agent's own summary of a held rollback (`agent.sh summary_for`, followed by the target version).
const _rollbackSummary = '이전 버전으로 되돌려요';

/// States in which a job may still finish after this screen stopped waiting.
const _going = {'queued', 'awaiting_approval', 'running'};

/// Owner-only recovery that used to need a terminal: roll back to the previous version (`rollback` job: owner
/// code, then the server approval code; the agent keeps its payout check) and download the sealed key copy
/// (`keys-backup` job: owner code; the copy is handed out once and recorded as kept off the server only when the
/// owner confirms it after the file was saved).
class ServerRecoveryCard extends StatefulWidget {
  const ServerRecoveryCard({super.key, required this.onQueued});
  final Future<void> Function() onQueued;

  @override
  State<ServerRecoveryCard> createState() => _ServerRecoveryCardState();
}

class _ServerRecoveryCardState extends State<ServerRecoveryCard> {
  final _pass = TextEditingController();
  final _again = TextEditingController();
  bool _busy = false;
  String? _passError;
  String? _note;
  bool _noteBad = false;

  @override
  void dispose() {
    _pass.dispose();
    _again.dispose();
    super.dispose();
  }

  void _say(String text, {bool bad = false}) => setState(() {
    _note = text;
    _noteBad = bad;
  });

  /// The note for a job's last answer: done, still going, unknown to old server tools, or the agent's reason.
  void _sayEnd(Json end, String done, String Function(String) failed) {
    final t = context.t, state = end['state'], why = jobReason(end, t);
    if (state == 'ok') {
      _say(done);
    } else if (state == 'queued' || state == 'unknown') {
      _say(why!, bad: true);
    } else if (_going.contains(state)) {
      _say(t.srStillRunning);
    } else if (state == 'rejected' && why == null) {
      _say(t.srOldTools, bad: true);
    } else {
      _say(failed(why ?? t.cRbDomainNoReason), bad: true);
    }
  }

  /// Runs [work] with the buttons off; any error shows in the note.
  Future<void> _guard(Future<void> Function() work) async {
    setState(() => _busy = true);
    try {
      await work();
    } catch (e) {
      if (mounted) _say(context.t.error(e), bad: true);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _rollback() async {
    final t = context.t, api = context.api;
    if (!await confirmAction(
      context,
      t.srRollbackButton,
      message: t.srRollbackConfirm,
    )) {
      return;
    }
    if (!mounted) return;
    await _guard(() async {
      final job = await runOwnerAction(
        context,
        (h) => queueHostJob(api, 'rollback', '', h),
        done: t.srRunning,
      );
      if (job is! Json || !mounted) return;
      await widget.onQueued();
      if (!mounted) return;
      _say(t.srRunning);
      final end = await _approveAndWait(api, job);
      if (end == null || !mounted) return;
      _sayEnd(end, t.srRollbackDone, t.srFailed);
      await widget.onQueued();
    });
  }

  /// Waits for the agent's hold, asks for the server approval code while the agent's own summary says this is
  /// a rollback, then waits for the end. Null (with the note set) when the owner stopped or the summary was not a rollback.
  Future<Json?> _approveAndWait(ApiClient api, Json job) async {
    final id = job['id'] as String;
    var j = await waitHostJob(api, id, until: {'awaiting_approval'});
    while (j['state'] == 'awaiting_approval') {
      final summary = await _serverSummary(api, j);
      if (!mounted) return null;
      final t = context.t;
      if (summary == null) {
        _say(t.srApprovalNoSummary, bad: true);
        return null;
      }
      final code = await askOwnerTotp(
        context,
        title: t.cRbApprovalTitle,
        body: '${t.cRbApprovalBody}\n\n$summary',
      );
      if (code == null) {
        if (mounted) _say(context.t.srApprovalCancelled);
        return null;
      }
      await api.post('/admin/host/jobs/$id/approve', {'code': code});
      // Still on hold after the agent's next look means a wrong code: ask again until the agent cancels.
      j = await waitHostJob(
        api,
        id,
        until: {'running'},
        limit: const Duration(seconds: 60),
      );
    }
    // A rollback backs up first (the agent allows it hours); after 30 minutes the job list keeps the result.
    if (j['state'] == 'running') {
      j = await waitHostJob(api, id, limit: const Duration(minutes: 30));
    }
    return j;
  }

  /// The root agent's summary of the held job, served by Caddy from the agent's public folder (the API cannot
  /// write it). Null unless it is the rollback summary.
  Future<String?> _serverSummary(ApiClient api, Json job) async {
    final token = job['progress_token'];
    if (token is! String) return null;
    for (var i = 0; i < 5; i++) {
      final p = await api.getServerFile('/ops-progress/$token');
      final s = p is Map ? p['summary'] : null;
      if (p is Map && p['state'] == 'awaiting_approval' && s is String) {
        return s.contains(_rollbackSummary) ? s : null;
      }
      await Future<void>.delayed(const Duration(seconds: 1));
    }
    return null;
  }

  Future<void> _keys() async {
    final t = context.t, api = context.api;
    final pass = _pass.text;
    final error = pass.characters.length < _minPass
        ? t.srKeysPassShort
        : pass != _again.text
        ? t.srKeysPassMismatch
        : null;
    setState(() => _passError = error);
    if (error != null) return;
    await _guard(() async {
      // Hex keeps any character safe in the agent's key=value job file.
      final arg = hex(utf8.encode(pass));
      final job = await runOwnerAction(
        context,
        (h) => queueHostJob(api, 'keys-backup', arg, h),
        done: t.srKeysRunning,
      );
      if (job is! Json || !mounted) return;
      _pass.clear();
      _again.clear();
      final id = job['id'] as String;
      final end = await waitHostJob(api, id);
      if (!mounted) return;
      if (end['state'] != 'ok') return _sayEnd(end, '', t.srKeysFailed);
      final text = await hostJobOutput(api, id);
      if (!mounted) return;
      if (text == null) {
        return _say(t.srKeysFailed(t.cRbDomainNoReason), bad: true);
      }
      final saved = await saveKeyCopy(
        base64.decode(text.trim()),
        'dahae-keys-${_stamp(DateTime.now())}.env.enc',
      );
      if (!mounted) return;
      if (!saved) return _say(t.srKeysSaveFailed, bad: true);
      // A browser can report a save that never happened: only the owner's word records the copy as kept.
      final kept = await confirmAction(
        context,
        t.srKeysKept,
        message: t.srKeysKeptConfirm,
      );
      if (!mounted) return;
      if (!kept) return _say(t.srKeysNotKept);
      await api.post('/admin/host/jobs/$id/kept', const <String, dynamic>{});
      if (!mounted) return;
      _say(t.srKeysDone);
      await widget.onQueued();
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(t.srTitle, style: context.text.labelLarge),
        const SizedBox(height: Space.xs),
        Text(t.srRollbackBody, style: context.text.bodySmall),
        const SizedBox(height: Space.sm),
        OutlinedButton(
          onPressed: _busy ? null : _rollback,
          child: Text(t.srRollbackButton),
        ),
        const SizedBox(height: Space.md),
        Text(t.srKeysTitle, style: context.text.bodyMedium),
        Text(t.srKeysBody, style: context.text.bodySmall),
        // Room for the filled fields' floating labels, which otherwise sit on the text above.
        const SizedBox(height: Space.sm),
        TextField(
          controller: _pass,
          obscureText: true,
          decoration: InputDecoration(labelText: t.srKeysPass),
        ),
        const SizedBox(height: Space.sm),
        TextField(
          controller: _again,
          obscureText: true,
          decoration: InputDecoration(
            labelText: t.srKeysPassAgain,
            errorText: _passError,
          ),
        ),
        const SizedBox(height: Space.sm),
        OutlinedButton(
          onPressed: _busy ? null : _keys,
          child: Text(t.srKeysButton),
        ),
        if (_note case final note?)
          Padding(
            padding: const EdgeInsets.only(top: Space.xs),
            child: Text(
              note,
              style: context.text.bodySmall?.copyWith(
                color: _noteBad ? context.colors.danger : null,
              ),
            ),
          ),
      ],
    );
  }
}

/// Lowercase hex of [bytes] (the agent's passphrase format).
@visibleForTesting
String hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

String _stamp(DateTime d) {
  String two(int n) => n.toString().padLeft(2, '0');
  return '${d.year}${two(d.month)}${two(d.day)}-${two(d.hour)}${two(d.minute)}${two(d.second)}';
}
