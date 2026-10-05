<script>
  import { createEventDispatcher } from 'svelte'
  import { api, setToken } from './api.js'

  const dispatch = createEventDispatcher()
  let username = ''
  let password = ''
  let error = ''

  async function submit() {
    error = ''
    try {
      const data = await api('/api/login', {
        method: 'POST',
        body: { username, password }
      })
      setToken(data.token)
      dispatch('login', data.user)
    } catch (e) {
      error = e.message
    }
  }
</script>

<div class="login-wrap">
  <div class="card">
    <h2>论文匿名送审系统</h2>
    <form on:submit|preventDefault={submit}>
      <input placeholder="用户名" bind:value={username} required />
      <input placeholder="密码" type="password" bind:value={password} required />
      {#if error}<div class="error">{error}</div>{/if}
      <button type="submit" style="width:100%">登录</button>
    </form>
    <div class="demo-accounts" style="margin-top:16px">
      演示账号（密码均为 123456）：<br />
      学生 student1 / student2 ｜ 导师 advisor1 / advisor2<br />
      秘书 secretary ｜ 评阅人 reviewer1 / reviewer2 / reviewer3
    </div>
  </div>
</div>
