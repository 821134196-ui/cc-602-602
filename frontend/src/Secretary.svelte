<script>
  import { onMount } from 'svelte'
  import { api, downloadFile } from './api.js'
  import { STATUS_TEXT, REC_TEXT, fmtTime } from './text.js'

  let theses = []
  let reviewers = []
  let error = ''
  let notice = ''

  // 每个论文的冻结表单状态：{ [thesisId]: {versionId, note} }
  let freezeForm = {}
  // 每个冻结轮次的分配选择：{ [freezeId]: [reviewerId...] }
  let assignSel = {}

  async function load() {
    theses = await api('/api/secretary/theses')
    for (const t of theses) {
      if (!freezeForm[t.id]) freezeForm[t.id] = { versionId: '', note: '' }
    }
  }

  onMount(async () => {
    try {
      reviewers = await api('/api/secretary/reviewers')
      await load()
    } catch (e) { error = e.message }
  })

  function allSubmitted(freeze) {
    return freeze.assignments.length > 0 && freeze.assignments.every((a) => a.review)
  }

  async function freeze(t) {
    error = ''; notice = ''
    const form = freezeForm[t.id] || {}
    if (!form.versionId) { error = '请选择要冻结的版本'; return }
    try {
      await api(`/api/secretary/theses/${t.id}/freeze`, {
        method: 'POST',
        body: { version_id: Number(form.versionId), anonymity_note: form.note || null }
      })
      notice = `《${t.title}》已冻结送审版本`
      freezeForm[t.id] = {}
      await load()
    } catch (e) { error = e.message }
  }

  async function assign(freezeId) {
    error = ''; notice = ''
    const ids = assignSel[freezeId] || []
    if (ids.length === 0) { error = '请选择评阅人'; return }
    try {
      await api(`/api/secretary/freezes/${freezeId}/assign`, {
        method: 'POST',
        body: { reviewer_ids: ids.map(Number) }
      })
      notice = '已分配评阅人（模拟邮件已发送）'
      assignSel[freezeId] = []
      await load()
    } catch (e) { error = e.message }
  }

  async function release(freezeId) {
    error = ''; notice = ''
    try {
      await api(`/api/secretary/freezes/${freezeId}/release`, { method: 'POST' })
      notice = '评阅意见已向学生与导师开放'
      await load()
    } catch (e) { error = e.message }
  }

  async function download(versionId, filename) {
    error = ''
    try { await downloadFile(versionId, filename) } catch (e) { error = e.message }
  }
</script>

{#if error}<div class="error">{error}</div>{/if}
{#if notice}<div class="ok">{notice}</div>{/if}

{#each theses as t}
  <div class="card">
    <h2>
      《{t.title}》 <span class="muted">T-{String(t.id).padStart(4, '0')}</span>
      <span class="badge status">{STATUS_TEXT[t.status] || t.status}</span>
    </h2>
    <div class="muted">学生：{t.student_name} ｜ 导师：{t.advisor_name}</div>

    <h3>稿件版本（送审版本以“实际送审稿”标记，避免混淆后续上传）</h3>
    <table>
      <thead>
        <tr><th>版本</th><th>文件名</th><th>上传时间</th><th>标记</th><th></th></tr>
      </thead>
      <tbody>
        {#each t.versions as v}
          <tr>
            <td>第 {v.version_no} 版</td>
            <td>{v.filename}</td>
            <td>{fmtTime(v.uploaded_at)}</td>
            <td>
              {#if v.id === t.latest_version_id}<span class="badge latest">最新上传稿</span>{/if}
              {#if v.is_review_version}<span class="badge review">实际送审稿</span>{/if}
            </td>
            <td><button class="ghost" on:click={() => download(v.id, v.filename)}>下载</button></td>
          </tr>
        {/each}
        {#if t.versions.length === 0}
          <tr><td colspan="5" class="muted">尚未上传稿件</td></tr>
        {/if}
      </tbody>
    </table>

    {#if t.status !== 'uploaded' && t.versions.length > 0}
      <details>
        <summary>匿名检查并冻结送审版本</summary>
        <div class="row" style="margin-top:8px">
          <select bind:value={freezeForm[t.id].versionId}>
            <option value="">选择版本</option>
            {#each t.versions as v}
              <option value={v.id} disabled={v.is_review_version}>
                第 {v.version_no} 版（{v.filename}）{v.is_review_version ? '（当前送审稿）' : ''}
              </option>
            {/each}
          </select>
          <input
            style="flex:1"
            placeholder="匿名检查结论，如：已确认无作者/导师身份信息"
            bind:value={freezeForm[t.id].note}
          />
          <button on:click={() => freeze(t)}>冻结此版本送审</button>
        </div>
        {#if t.latest_version_id !== t.review_version_id && t.review_version_id}
          <div class="muted" style="margin-top:4px">
            注意：存在比当前送审稿更新的上传版本，如需要可冻结新版本重新送审。
          </div>
        {/if}
      </details>
    {:else if t.status === 'uploaded'}
      <div class="muted">导师尚未确认，暂不能冻结送审。</div>
    {/if}

    <h3>送审与分配记录</h3>
    {#each t.freezes as f}
      <div class="review-box">
        <div>
          <strong>送审轮次 #{f.id}</strong>：第 {f.version_no} 版
          {#if f.released_at}<span class="badge review">意见已开放</span>{/if}
        </div>
        <div class="muted">
          冻结时间：{fmtTime(f.frozen_at)} ｜ 匿名检查：{f.anonymity_note || '—'}
          {#if f.released_at}｜ 开放时间：{fmtTime(f.released_at)}{/if}
        </div>
        <table style="margin-top:8px">
          <thead>
            <tr><th>评阅人</th><th>分配时间</th><th>评阅结论</th><th>意见</th></tr>
          </thead>
          <tbody>
            {#each f.assignments as a}
              <tr>
                <td>{a.reviewer_name}</td>
                <td>{fmtTime(a.assigned_at)}</td>
                <td>
                  {#if a.review}
                    {REC_TEXT[a.review.recommendation] || a.review.recommendation}
                    {#if a.review.score != null}（{a.review.score} 分）{/if}
                  {:else}
                    <span class="muted">未提交</span>
                  {/if}
                </td>
                <td>{a.review ? a.review.comment : '—'}</td>
              </tr>
            {/each}
            {#if f.assignments.length === 0}
              <tr><td colspan="4" class="muted">尚未分配评阅人</td></tr>
            {/if}
          </tbody>
        </table>
        {#if !f.released_at}
          <div class="row" style="margin-top:8px">
            <select multiple size="3" style="min-width:160px"
              on:change={(e) => (assignSel[f.id] = [...e.target.selectedOptions].map((o) => o.value))}>
              {#each reviewers as r}
                <option value={r.id}>{r.display_name}</option>
              {/each}
            </select>
            <button on:click={() => assign(f.id)}>分配评阅人</button>
            <button on:click={() => release(f.id)} disabled={!allSubmitted(f)}>
              开放评阅意见
            </button>
          </div>
          {#if !allSubmitted(f) && f.assignments.length > 0}
            <div class="muted">全部评阅人提交后才能开放意见。</div>
          {/if}
        {/if}
      </div>
    {/each}
    {#if t.freezes.length === 0}
      <div class="muted">尚无送审记录。</div>
    {/if}
  </div>
{/each}

{#if theses.length === 0}
  <div class="card muted">暂无论文。</div>
{/if}
