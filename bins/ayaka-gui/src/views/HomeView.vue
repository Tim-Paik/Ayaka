<script setup lang="ts">
import { conv_src, info, next_run, start_new } from '../interop'
import { platform, Platform } from '@tauri-apps/api/os'
</script>

<script lang="ts">
export default {
    emits: ["quit"],
    data() {
        return {
            title: "",
            bg: undefined as string | undefined,
            os: "win32" as Platform
        }
    },
    async created() {
        const res = await info()
        this.title = res.title
        this.bg = conv_src(res.props.bg)
        this.os = await platform()
    },
    methods: {
        async new_game() {
            await start_new()
            if (await next_run()) {
                this.$router.replace("/game")
            }
        }
    }
}
</script>

<template>
    <img class="background" :src="bg">
    <div class="content-full bg-body backboard-bg"></div>
    <div class="content">
        <div class="d-grid gap-4 col-4 mx-auto">
            <h1>{{ title }}</h1>
            <button class="btn btn-primary" @click="new_game">{{ $t("newGame") }}</button>
            <router-link class="btn btn-primary" to="/records/load">{{ $t("loadRecords") }}</router-link>
            <router-link class="btn btn-primary" to="/settings">{{ $t("settings") }}</router-link>
            <router-link class="btn btn-primary" to="/about">{{ $t("about") }}</router-link>
            <button v-if='os != "ios" && os != "android"' class="btn btn-primary" @click="$emit('quit')">{{
                $t("quit") }}</button>
        </div>
    </div>
</template>

<style>
.backboard-bg {
    opacity: 0.5;
}
</style>
