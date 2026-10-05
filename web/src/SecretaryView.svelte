<script>
  import { onMount } from 'svelte'
  import { api, statusLabel, fmtTime } from './api.js'

  let theses = $state([])
  let detail = $state(null)
  let reviewers = $state([])
  let emails = $state([])
  let tab = $state('theses')
  let error = $state('')
  let msg = $state('')

  // 每个轮次的操作状态
  let checkComment = $state({})
  let scanResult = $state({})
  let selected = $state({})

  async function load() {
    const data = await api('/api/theses')
    theses = data.theses
    if (detail) detail = await api(`/api/theses/${detail.id}`)
  }
  async function select(id) {
    error = ''; msg = ''
    detail = await api(`/api/theses/${id}`)
    if (!reviewers.length) reviewers = (await api('/api/secretary/reviewers')).reviewers
  }
  async function loadEmails() {
    emails = (await api('/api/emails')).emails
  }
  async function run(fn, okMsg) {
    error = ''; msg = ''
    try { await fn(); msg = okMsg; await load() } catch (e) { error = e.message }
  }

  const createRound = (id) => run(() => api(`/api/theses/${id}/rounds`, { method: 'POST', body: {} }), '已创建送审轮次并冻结当前稿件')
  const check = (rid, passed) => run(() => api(`/api/rounds/${rid}/check`, { method: 'POST', body: { passed, comment: checkComment[rid] || '' } }), passed ? '匿名检查通过，稿件已冻结' : '已退回，学生需修改后重新上传')
  const assign = (rid) => run(() => api(`/api/rounds/${rid}/assign`, { method: 'POST', body: { reviewer_ids: selected[rid] || [] } }), '已分配评阅人并发出通知')
  const release = (rid) => run(() => api(`/api/rounds/${rid}/release`, { method: 'POST', body: {} }), '评阅意见已向学生开放')

  async function scan(rid) {
    error = ''
    try { scanResult[rid] = await api(`/api/rounds/${rid}/scan`) } catch (e) { error = e.message }
  }
  function toggle(rid, id, ev) {
    const cur = new Set(selected[rid] || [])
    ev.target.checked ? cur.add(id) : cur.delete(id)
    selected[rid] = [...cur]
  }

  onMount(load)
</script>

<div class="tabs">
  <button class:ghost={tab !== 'theses'} onclick={() => (tab = 'theses')}>论文管理</button>
  <button class:ghost={tab !== 'emails'} onclick={() => { tab = 'emails'; loadEmails() }}>模拟邮件记录</button>
</div>

{#if tab === 'emails'}
  <div class="card">
    <h2>模拟邮件（系统“发出”的通知）</h2>
    <table>
      <thead><tr><th>时间</th><th>收件人ID</th><th>主题</th><th>内容</th></tr></thead>
      <tbody>
        {#each emails as m}
          <tr><td>{fmtTime(m.sent_at)}</td><td>{m.to_user_id}</td><td>{m.subject}</td><td>{m.body}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <div class="card">
    <h2>全部论文</h2>
    <table>
      <thead><tr><th>题目</th><th>学生</th><th>导师</th><th>状态</th><th></th></tr></thead>
      <tbody>
        {#each theses as t}
          <tr>
            <td>{t.title}</td><td>{t.student_name}</td><td>{t.advisor_name}</td>
            <td>{statusLabel(t.status)}</td>
            <td><button class="ghost" onclick={() => select(t.id)}>管理</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if detail}
    <div class="card">
      <h2>{detail.title} <span class="badge">{statusLabel(detail.status)}</span></h2>
      <p class="muted">学生：{detail.student_name} · 导师：{detail.advisor_name}</p>
      {#if msg}<p class="ok">{msg}</p>{/if}
      {#if error}<p class="error">{error}</p>{/if}

      <h3>稿件版本</h3>
      <table>
        <thead><tr><th>版本</th><th>文件名</th><th>上传时间</th><th>标记</th><th></th></tr></thead>
        <tbody>
          {#each detail.versions as v}
            <tr>
              <td>第 {v.seq} 版</td><td>{v.orig_filename}</td><td>{fmtTime(v.uploaded_at)}</td>
              <td>
                {#if detail.review_version && detail.review_version.version_id === v.id}
                  <span class="badge frozen">实际送审稿</span>
                {/if}
                {#if detail.versions[0].id === v.id}<span class="badge">最新上传</span>{/if}
              </td>
              <td><a href={`/api/files/${v.id}/download`} download>下载</a></td>
            </tr>
          {/each}
        </tbody>
      </table>

      {#if detail.status === 'confirmed'}
        <button onclick={() => createRound(detail.id)}>创建送审轮次（冻结当前稿）</button>
      {/if}
      {#if detail.review_version && detail.versions.length && detail.versions[0].id !== detail.review_version.version_id && ['frozen','in_review','completed','released'].includes(detail.review_version.round_status)}
        <div class="reassign">
          <p>学生已上传更新的稿件（第 {detail.versions[0].seq} 版），当前送审仍使用第 {detail.review_version.seq} 版。</p>
          <button class="danger" onclick={() => createRound(detail.id)}>以最新稿重新送审（作废旧轮次）</button>
        </div>
      {/if}

      {#each detail.rounds as r}
        <div class="round">
          <h3>第 {r.round_no} 轮 <span class="badge">{statusLabel(r.status)}</span>
            <span class="muted">冻结版本：第 {r.version_seq} 版（{r.version_filename}）</span></h3>

          {#if r.status === 'checking'}
            <div class="op">
              <button class="ghost" onclick={() => scan(r.id)}>扫描匿名内容</button>
              {#if scanResult[r.id]}
                {#if scanResult[r.id].clean}
                  <span class="ok">未发现学生/导师姓名</span>
                {:else}
                  <span class="error">发现疑似敏感词：
                    {scanResult[r.id].findings.map((f) => `${f.term}×${f.count}`).join('、')}</span>
                {/if}
              {/if}
            </div>
            <div class="op">
              <input placeholder="检查意见（退回时必填原因）" bind:value={checkComment[r.id]} />
              <button onclick={() => check(r.id, true)}>检查通过并冻结</button>
              <button class="danger" onclick={() => check(r.id, false)}>退回</button>
            </div>
          {/if}

          {#if r.status === 'frozen'}
            <div class="op">
              <span>选择评阅人：</span>
              {#each reviewers as rv}
                <label class="chk">
                  <input type="checkbox" onchange={(e) => toggle(r.id, rv.id, e)} />
                  {rv.display_name}
                </label>
              {/each}
              <button onclick={() => assign(r.id)} disabled={!(selected[r.id] || []).length}>分配</button>
            </div>
          {/if}

          {#if r.assignments && r.assignments.length}
            <h4>分配记录（仅秘书可见评阅人身份）</h4>
            <table>
              <thead><tr><th>别名</th><th>评阅人</th><th>分配时间</th><th>状态</th><th>意见</th></tr></thead>
              <tbody>
                {#each r.assignments as a}
                  <tr>
                    <td>{a.alias}</td><td>{a.reviewer_name}</td><td>{fmtTime(a.assigned_at)}</td>
                    <td>{a.report ? '已提交' : '待提交'}</td>
                    <td>
                      {#if a.report}
                        {a.report.score ?? '-'} 分 · {a.report.recommendation ?? ''} · {a.report.comment}
                      {:else}-{/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}

          {#if r.status === 'completed'}
            <button onclick={() => release(r.id)}>归档并向学生开放意见</button>
          {/if}
          {#if r.status === 'released'}
            <p class="ok">意见已于 {fmtTime(r.released_at)} 向学生开放。</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .tabs { display: flex; gap: 8px; margin-bottom: 16px; }
  .round { border-top: 1px solid #eee; padding-top: 10px; margin-top: 14px; }
  .op { display: flex; gap: 10px; align-items: center; margin: 10px 0; flex-wrap: wrap; }
  .chk { display: flex; gap: 4px; align-items: center; }
  .reassign { background: #fff8e6; border: 1px solid #f0d98c; border-radius: 8px; padding: 10px 14px; margin: 12px 0; }
  a { color: #24509e; }
</style>
