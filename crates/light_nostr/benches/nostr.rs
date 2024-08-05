use criterion::{criterion_group, criterion_main, Criterion};
use light_nostr::{Event, Filter};

fn matches(c: &mut Criterion) {
    let filter = r#"{"kinds":[1,6],"authors":["2934f677475d7880cbee2e1d9476eca0b4c8109f867f62cc710886caa5c320a5","79c2cae114ea28a981e7559b4fe7854a473521a8d22a66bbab9fa248eb820ff6","818a39b5f164235f86254b12ca586efccc1f95e98b45cb1c91c71dc5d9486dda","d46ede11e96ab2c3631e514f8b5481f98a1ca3259e5fa7207d2de3554b06bd1d","65fb9ac2ccb5ec48e6eb55916a3d24668e938db66b05141cd2be69fd6ccd8098","9475f17943f64bb97bb0c553292c793b94f89a279adc9216a7c7b55f2fecf287","9a1bfb35242384b3fd5a89fb2fdd1364244e0d538b9906e057eb730bde67a8bf","9841e1f82d25a4c44f920c288672a0f9e50a2df31aa79695296aa2baba727630","e9f2673821bc3ba4c3134d10104714155a89c9bc0ea8b464b55f295362b6384a","68d81165918100b7da43fc28f7d1fc12554466e1115886b9e7bb326f65ec4272","a77fb9c707df14f8fbbdbcc2193d98804b2533b4a9308e8f330fdabd3e0033dd","18fdc3711744150491bba2b2a94aa9a5caafdd8ee6a15dc7e70bcb4b3f7241df","9dbe5d7d9a502e44fb0b0b4a01bcf991bcf03b26615e06f3644fe2ae3be88df9","97f914c478d49195af6b3eedd4d0b48b3726a00d11d85785a1d2e351b626f8de","ba30adf1fbf44260c306bf7ec60ff94c07f9412546fc35153a8ce1b160787155","dbc7d51f09085b9f6bd483c79dec43173e9dc70025c1b88c1f987023dff20132","6369fb82bec54226bf09bb365b6f0d71b16c41f56e7edfd4f97de23f9c3281da","af875afdd024625f7178a2e73974bae71d415f1029bbd6435494712a881b9468","6be38f8c63df7dbf84db7ec4a6e6fbbd8d19dca3b980efad18585c46f04b26f9","dd7fd697f7546f54f3c949eadc92f6ad77e05352750b2e92298aa5c408b9f42f","28b1e9729c1b9287d2f7df221bd1da08e829a4603cb0f847cb72b531a1fc9dcf","cd3fad2798b3728f82d9369d9fe833618277a1a2a070a256f1780a259a9ac126","ad6ca9546e8693927872462c486a8f31bbe131afe8f15afccb78699a4ecb5c3c","26130449c250be4ad281889e399d58825275ed3347b7ff8e7590a19894170485","b85dc022de71f09c708395bc1763f14275454208980e4fa7778f5953957cbff4","86d2439a17531ad9e2902640e2cf22957ae61998052887eb35fad71bcc303891","b2a0a51cebf0f93dd07b71e21fb500a12672eaaf5736ad8a8b197800eb1a9b38","266815e0c9210dfa324c6cba3573b14bee49da4209a9456f9484e5106cd408a5","b0adc50032fb926d81ae35a732bdaabdee63e5d50807a8ed8d5021ead0278303","28fbcb6df834dab5defd8cad82806bdad0c7d0563fe960ecb7f3384d96cc9b54","22cc175329adf36f8272df5f448b0a4139370540a45b252f91a94656337c6433","08071429168b213182c5b3920060d8d36987486fb930f9fc42284949a4aa634b","cdafe99cd0d2ce2b98f5768523dc90be73289fe7b6d0b0eb19d09b893d00eefb","50e1d0847c53cfcec296d0b8ef00a97b2216b7bc580807baf914b275d777a29d","f91448eb6895ddce6b14bdac9058c11838541c30537bf4eb0d96507597d4b54f","3272ff5f7c575da0c8dc013c6a9abc02ad4488c21bc9b5c8dc9a7df52fbaec6f","77912c8893829162481ff365691b5a61112a9e21f9428abc4d690c62d480792a","a1295368930c1b376c0ef1d14892184212d67dd7c2fc7d1b9b188431dbfbb530","8caba603a7c1d7e87aa3e20364665acbb88ac77101aee4c1c458c7fdb6c5a65c","7d533ee3dc090499b3975352b9b57607965cd3be03b6af3d8754f4c39c6f8c27","8fa125b4f2bdd1db4181c5ce7f76a1194ff60307659ae414455fcb1d65f01b4e","cfb74752c0ccd0585efbbf59f62942ebb1d57b6c6e13e512deb03e833eee86c5","e53b78a6f4e1919d63f25a1d33f5c3d46f804a67f9266d2a64ca024be9fbced0","ce653ea853dd1924fbfca09dd86a53dbddf03a2185ce521b906db3f680b66fa7","f1c6e2a13d6ba0b9564195318039e5132f562c6814ece09cf386f892735e04c8","d21d621fb2357fa3fb0d560c92f4f3b5c97d2c8c2a53132516edf59161a9eee5","8052671368a38bc9d673f1e9d3d83fa29adf83083eef1a7a640da8a4edec5676","e778a074c2882d3b23b9cc1f12507684e4f26a60ba7473c7ed5fca8a6121a36a","07c696be16f116bec2513af501c5bec793d1e34281087ba337401c186c4b412d","267cf27f44113f537d3ba323d8a5232ffc7c485ac3fa1627274c4bcaae88dc9a","d7bed24efd4d1ef9eb96601954bf4276ab9fdffd3d8f835186707a1a72eb1996","0461fcbecc4c3374439932d6b8f11269ccdb7cc973ad7a50ae362db135a474dd","05fe30aa04d5c1856ab948916ce570d63c46b97b996771a471b2045756d664cb","4ddb7d1be8dfcb0482dad8147d91f3a154af2712cdf443bfe35dc0cc9a8563b5","440f5f7d38636b2676b9a7e914244ffd92d950cd890241d052b8f2982de99be5","a114a01ded64e4cf71cb4f06b7ced8ca5bf6701045696cc3471ce263817eb59d","8e6ecbc34dc91445ec899d1b3c1e42a065f79329d52970e45404b144d31cb2cc","b68f46a12aaac98649bbae4472653e2e67d2a31c65d8a814f35c72302858eca0","7784a0cffd511dc82545a930e976fd930701ba4f66a02b4a2705674cc4c7f50c","27d626252b77ce249f90058b3228cf62a30bdaa67e8791e3d2eb6a1c04be8d71","4ee8e9dff045f203c3203db78333199d398c77d7972e8a8d16ce5d92b8923635","b6a649a8f70f09cdeb577649eb6df7e8898c4fcd96b953c09d3b5cfe47816bb8","d9e363231e9a838938307aa570bf5bbdbb88d3338d83d31c04b7a28625de7761","3530288e089c69e66f20dcc855757cee807da4d5e55558f29e5948ea29e92098","befa0d35d6485fd92b4632192f2a67615c152bd2cf5ffc09cd996d2e483436b4","9dfa20aec379dfe71de04ea6f475e227e6e8e5ec4454f1cac32d1c222b72700d","0ea95cb28f3a568231dbd07a06d3b068a0ef8ac9a349502a4d8f772ed3b3baf3","8559b2440a5b774f226fe28df8ea37e4c55eb8f98412c85802a5d06ae6675b22","dbf6f6efae0173423554cab58709c0744be59d644dc18ac67351aa5342455450","570e5fe50c4df0580530bd15e9f0879d506b5fa635378a13d46ba5b6a22b1e32","3d5a2496fa24aa449ac4418f548bb389ba9fc1db746add5022e17574fd4b9bb6","fc7085c383ba71745704bdc1c6efcf7fab0197501de598c5e6c537ac0b32a4cb","14b438bacb99b135a7527a2a0af8a12d5b37fbd2383b9492d9718a5753f9c9cb","0c085bcac0642cd66d2e451ac24d976707fafc834a2b720dd1f8d7b43ef14489","3d21d4178885b877976ac1859cbe21d87ce667b6599e2013bb6f823e8b8ab88d","2872ab118f7acd64cab3982a7235b9c633b5fffd3996f8dfc0e96c7172e164a7","1000a4205517276b2ee447be9dee0085e0bdfd939dc5438f827ac897b9e042e8","6f8774e694772079359ed2c247de238c5a25e2df5fb8e66a10ad062c6af67159","bb0ece96566e0bd3869ac09dc4ae6ff42a8a4bdd457faa1e2221753e3434df61","d35e5db896121725ce3a52d1f5b593e815b312564aeaa134809aed5aefa69d28","5bb738dba8eb51d5bc5f4d42b4a07639594594d941cf785207ddc0c4fa1b276c"],"limit":20,"since":1722855527}"#;
    let f: Filter = serde_json::from_str(filter).unwrap();
    // cspell: disable
    let e = r#"[
    {"id":"3bf56fe8a735837a6050e686b1ade0064c356d918d55538f18b17c9fa69db088","pubkey":"dbf6f6efae0173423554cab58709c0744be59d644dc18ac67351aa5342455450","created_at":1722867813,"kind":1,"tags":[["p","43f177cd7dadd17426aff4a7ee82b59bc201e8bb144294192872340e9f755b92"],["e","2dcef998b4adc4758f6d1b699e8aff90bb04647031739ce47d15aea93a38f872","","root","ba31f050a66d6aee73fb258405a44fcffa662e24784e6650944f8fc2d3089427"],["e","af55b791bed160da6162b44d41aa1f5316f2337af462a0e195856f30003b0d10","","reply","43f177cd7dadd17426aff4a7ee82b59bc201e8bb144294192872340e9f755b92"]],"content":"We are already bridging your account.","sig":"735dc4aed80ee9610c878919f162500301a5deeb2a119897925643a4851fdc5ccd46850b22c9d00565f38946ef8338de52d3d59168108b140b958ee408a19a31"},
{"id":"ead8574b0f3438d6831d75d6971540935a739f9eb76dce0283f1c9970c9ecb7b","pubkey":"dbf6f6efae0173423554cab58709c0744be59d644dc18ac67351aa5342455450","created_at":1722867757,"kind":1,"tags":[["p","ba31f050a66d6aee73fb258405a44fcffa662e24784e6650944f8fc2d3089427"],["e","2dcef998b4adc4758f6d1b699e8aff90bb04647031739ce47d15aea93a38f872","","root","ba31f050a66d6aee73fb258405a44fcffa662e24784e6650944f8fc2d3089427"]],"content":"We are already bridging your account.","sig":"5aa53e804ff3435aed1a28e61fdea1a8f770383901057507e74f96803eb350515b6cbd3f34f219be4385d9f8423b0c145fcdd60ad92df3d43fea90b5edba9045"},
{"id":"0be4f2bebfd6b36eaedea1a034e82872d5207894ac10b9887b3719b088065b2c","pubkey":"ad6ca9546e8693927872462c486a8f31bbe131afe8f15afccb78699a4ecb5c3c","created_at":1722867168,"kind":1,"tags":[["emoji","okurachanjk_smile","https://ms.secinet.jp/d/fb4bb721-a3fd-4dce-bfee-d8b598022dce.webp"],["proxy","https://misskey.secinet.jp/notes/9wkmxwfu6y","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.secinet.jp/notes/9wkmxwfu6y","pink.momostr"],["-"]],"content":"いいぞ京本〜​:okurachanjk_smile:​\nhttps://x.com/taiga_kyomo33/status/1820404618029068565?s=46","sig":"e5f47035d01e812444d890d2bed4e3b093008788a0f60ad0415eaa4524d38dcbd996e300cbaf90298de631bf6f5d35f3f9572dd21a61440cfbe1911e41243a66"},
{"id":"72ed111f22d78f42002c3a59468866498ddde6f55dc665cf02ca62f61b3d153f","pubkey":"af875afdd024625f7178a2e73974bae71d415f1029bbd6435494712a881b9468","created_at":1722866377,"kind":1,"tags":[["proxy","https://misskey.io/notes/9wkmgxzo8vy204x9","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkmgxzo8vy204x9","pink.momostr"],["-"]],"content":"今日のオタ活\n\nゲーム\n・伝説のスタフィー2　9-3まで。明日は最終ステージまでいけるかな？　レッシーこは迷いやすいよなぁ\n\nアニメ\n・しかのこのこのここしたんたん　こしたんも猫山田さんもチョロすぎる、動物園カオス。鹿の大行列とかさぁｗ　ギャグアニメじゃなかったら体育館爆破がシャレにならなかったな！\n・VTuberなんだが配信切り忘れたら伝説になってた　体の一部とはいえピー音がｗ　4期生たちも個性ありすぎ！　で、淡雪ちゃんのセンシティブイラスト紹介は草。淡雪ちゃんの黒歴史掘り起こしもな！\n・菜なれ花なれ　お店の閉店を阻止しようと奮闘する杏那ちゃんを見て『白い砂のアクアトープ』の第1クールを思い出す。最後までハラハラしながら見てた……！　良かった！！\n・異世界ゆるり紀行 ～子育てしながら冒険者します～　猫捜しはほのぼのしたし、依頼を終えた後も双子ちゃんが「やだ～捜す～！」って言うの、とても子どもらしい。ゴブリン退治の過程で念話を覚えたワンちゃん！　双子ちゃんが森へ放置された経緯も明らかになって次回が気になる\n\n・戦場のヴァルキュリア　21話～23話　アリシアちゃん、ついにヴァルキュリア人として覚醒。あれ……アリシアちゃんとセルベリアさんって同じ孤児院の出身なのか……？　だとしたら辛すぎる……。ゲーム版だとヴァルキュリア人は崇められていたらしいけど、アニメ版だと化け物扱い……。\n・異世界スーサイドスクワット　弾薬補充の為に戻ったらドラゴンが！　ドラゴンとのバトルはカメラアングルが目まぐるしく動いて迫力アリアリだし、役割分担もあって満足！！　ラスト何があったん……\n・真夜中ぱんチ　真夜中の無人島！　この設定ならでは、だよね！　カニ相手にギャンブルする十景さんｗｗｗどんどん追い詰められる皆……ホント、危機一髪だったなぁって……","sig":"42baecc283956c0de6267fa2519b90667602f4f35116840615d2ee07a25cbe175590580e833471118c882c51b5bb3bba2f1ad169b53473835e853f48ffd21607"},
{"id":"cdaca43c35173e3213875dec19d73c4f526622cdcebb0e64bb9e20287607a0bc","pubkey":"2872ab118f7acd64cab3982a7235b9c633b5fffd3996f8dfc0e96c7172e164a7","created_at":1722865391,"kind":1,"tags":[["proxy","https://mstdn.jp/@aimiteno_c/112909706309208831","web"],["t","男子バレー"],["proxy","https://mstdn.jp/users/aimiteno_c/statuses/112909706309208831","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://mstdn.jp/users/aimiteno_c/statuses/112909706309208831","pink.momostr"],["-"]],"content":"みんなよくやったよ。ええ試合見せてもろてオバちゃんは嬉しいよ。ありがとう。\n (TДT)(TДT)(TДT)\n#男子バレー","sig":"4d348459d9fb06d65d13e5ee20315c1e5e8484cb7dace8c9fdc203baed525cdc81b828f08d35228bc5d4d58bc8c68b30719f26b1c997ef9dd910e119c0db80d9"},
{"id":"34b82bbbd25e6e0556418ed043bc678ddd2051b6856a0c8545bfbd86321cfe3c","pubkey":"0c085bcac0642cd66d2e451ac24d976707fafc834a2b720dd1f8d7b43ef14489","created_at":1722865344,"kind":1,"tags":[["emoji","ablobcatcry","https://media.misskey.cloud/files/8ed3f814-7467-47a8-a952-e8f607e3a949.png"],["proxy","https://misskey.cloud/notes/9wklutevfg","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.cloud/notes/9wklutevfg","pink.momostr"],["-"]],"content":"解説の人泣いてる​:ablobcatcry:​","sig":"eb85757a67b7042cc4a7a6aabd0bd58e360ea13ce64a2fddcff67b3a516febe5831bb58b57202afa36b2fab15732d419c34392defbd190150c5a5d2ffbea9125"},
{"id":"f6930361a4be6d849a4c61613a2a23c163d166009165cec3f87d22465b4bc493","pubkey":"0c085bcac0642cd66d2e451ac24d976707fafc834a2b720dd1f8d7b43ef14489","created_at":1722865263,"kind":1,"tags":[["proxy","https://misskey.cloud/notes/9wklt29fdz","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.cloud/notes/9wklt29fdz","pink.momostr"],["-"]],"content":"あちゃー","sig":"be85306c1f40277b985175cc7b64753162284826a20e24b171a57fb2c5a885e3144b9a7edc7daa3176af050cc1abd1b15c2858ef9a656286fc0c97c58607d3e9"},
{"id":"11800c8dc517aaa3466f843bac86c4b885ab70b4b1fd1998d6377d5ae5f319a7","pubkey":"b85dc022de71f09c708395bc1763f14275454208980e4fa7778f5953957cbff4","created_at":1722865198,"kind":6,"tags":[["e","0621cc250bf423efab3d36fbbc37bd04c960b19e73413b7ec01fd64c5f742159","86d2439a17531ad9e2902640e2cf22957ae61998052887eb35fad71bcc303891"],["p","86d2439a17531ad9e2902640e2cf22957ae61998052887eb35fad71bcc303891"],["proxy","https://fosstodon.org/users/fedora/statuses/112909693645915758/activity","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://fosstodon.org/users/fedora/statuses/112909693645915758/activity","pink.momostr"],["-"]],"content":"","sig":"bd71a45f63f32a5a8183a0cd8599d857d4702b99e818929fc7e3d63f415f40dc939750a9f3766c6b453f05932ddf99b43be8c11fff7c2a3c080b7ae2d9caf902"},
{"id":"a7b069044136b8b4b1c3ae43aa1c9ff9d35694e3ff46b11bc94156dcc6b89f3f","pubkey":"2872ab118f7acd64cab3982a7235b9c633b5fffd3996f8dfc0e96c7172e164a7","created_at":1722865179,"kind":1,"tags":[["proxy","https://mstdn.jp/@aimiteno_c/112909692411305509","web"],["t","男子バレー"],["proxy","https://mstdn.jp/users/aimiteno_c/statuses/112909692411305509","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://mstdn.jp/users/aimiteno_c/statuses/112909692411305509","pink.momostr"],["-"]],"content":"うぐぐ　しんどい　やめて　心臓に悪い\n #男子バレー","sig":"f6c7823dd502304eeef5bd72ca55f45c444a7be48c50cbe3edc95971f0217f9f65dcb8a44e76034331877ddca9eac6a2891d514980e053bee809a29331bb699c"},
{"id":"c2817e2d4474a2c10f46766035fc21896dd80b7fafb57e5324d9de423808cb9b","pubkey":"0c085bcac0642cd66d2e451ac24d976707fafc834a2b720dd1f8d7b43ef14489","created_at":1722865111,"kind":1,"tags":[["proxy","https://misskey.cloud/notes/9wklpte5ct","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.cloud/notes/9wklpte5ct","pink.momostr"],["-"]],"content":"男バレあちぃ","sig":"598b66b4990068ebdde90f45f3151e783c018722b1e5f9dc31b6ba8cb75a5d1f7c1de870b08d049882f784de45dfefc3e9327dc8125972b5b138dd7107aaabc0"},
{"id":"60da12ca045db62baad795fa3cf6fa8efc7b15c192adae8cdd6f78bfba5d7f76","pubkey":"9a1bfb35242384b3fd5a89fb2fdd1364244e0d538b9906e057eb730bde67a8bf","created_at":1722864336,"kind":6,"tags":[["e","97c28dc5585fa2fea7ad679faf6d8991d4b8b5287e66ec56b77a52b7e84ccdf0","2a9380fbb77c2266aa5af225e412179fe78539e4912f267bb979480014f65085"],["p","2a9380fbb77c2266aa5af225e412179fe78539e4912f267bb979480014f65085"],["proxy","https://misskey.io/notes/9wkl97ob4rxa0c6u/activity","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkl97ob4rxa0c6u/activity","pink.momostr"],["-"]],"content":"","sig":"1487fe230d573fcf450935444b8a5e10c4717dd70da6093826aebceb4ce8ed08151ea210727d37dfa521999131cb75bcf5c80d83d26d7e71a436932c8e7e7644"},
{"id":"100c654c23b7291e58d5cd336c5246ece680bf9627b7d8a0612c11830d06a74c","pubkey":"0c085bcac0642cd66d2e451ac24d976707fafc834a2b720dd1f8d7b43ef14489","created_at":1722864322,"kind":1,"tags":[["proxy","https://misskey.cloud/notes/9wkl8wew19","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.cloud/notes/9wkl8wew19","pink.momostr"],["-"]],"content":"テレビ付けたら男子バレーを良いところ","sig":"52b3dd55ae41c1b04a3fa3460512d9138722a97e95e3344edbf53550b87c901e2d76bf9b66266e3cff16e72be44a2ecc869a9543b6ce37b99ff8ab8f295cef9c"},
{"id":"ccade8f455464c82eaa87cf6047750d6c0cfcc37b2c47dd4865bc4d769d35482","pubkey":"cd3fad2798b3728f82d9369d9fe833618277a1a2a070a256f1780a259a9ac126","created_at":1722864121,"kind":1,"tags":[["proxy","https://mastodon.social/@zig_discussions/112909623094711197","web"],["t","ziglang"],["t","programming"],["proxy","https://mastodon.social/users/zig_discussions/statuses/112909623094711197","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://mastodon.social/users/zig_discussions/statuses/112909623094711197","pink.momostr"],["-"]],"content":"Looking for collaborations\n\nhttps://github.com/basilysf1709/distributed-systems\n\nDiscussions: https://discu.eu/q/https://github.com/basilysf1709/distributed-systems\n\n#programming #ziglang","sig":"a8649da7ce7f40c071ecbba8d51dbcfb685db794057eea1bbf40549b073153fbf49909b770908f579bad3b6fbbc23509b69b803f107b3547899dd68476c866fb"},
{"id":"730e7d1e738ef62169677a5a8a9d4f3cee1dc4f75320be329b9f37b32430b03b","pubkey":"b68f46a12aaac98649bbae4472653e2e67d2a31c65d8a814f35c72302858eca0","created_at":1722863858,"kind":1,"tags":[["proxy","https://misskey.io/notes/9wkkyy5ppmik0b4y","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkkyy5ppmik0b4y","pink.momostr"],["-"]],"content":"みなさま花火の写真上げてて素敵","sig":"c269d8e2240d1cc72543e91a392f4d4df2db529c68a13897865ac5dadea11d2bf9c33e16764bf2727a3cc471cda16bb6f6a5bd4f99f86f1f57ad0dfc36c8551b"},
{"id":"bbdca95d68cdcbfa8dcb1fc33e055b8620962d990cdf30fb3105022723a89a96","pubkey":"b68f46a12aaac98649bbae4472653e2e67d2a31c65d8a814f35c72302858eca0","created_at":1722863665,"kind":1,"tags":[["proxy","https://misskey.io/notes/9wkkutziabng0ae1","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkkutziabng0ae1","pink.momostr"],["-"]],"content":"蒸留酒を薄めたやつ飲用してる","sig":"1dd123a4878d2bfe2c3d4c2262611f4130f7bc5cb42777e4cc6ac5f93bfa7105d9735487ebd45fd5aaf646a7d20e72070fa48586aea77b7412a0b609f6af34c0"},
{"id":"b17d4c8a46d6d81a916320c28128cf5cfa3f4003abc51bb3205f58e02ccbd35a","pubkey":"e53b78a6f4e1919d63f25a1d33f5c3d46f804a67f9266d2a64ca024be9fbced0","created_at":1722862978,"kind":1,"tags":[["proxy","https://misskey.io/notes/9wkkg3msdlyx058i","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkkg3msdlyx058i","pink.momostr"],["-"]],"content":"眠いけど荷物が届くまで起きてなきゃ…","sig":"4888b1a3fef85fc6f851d04e118d6b0a9e3f0abde2deadd8fad79bdb9c4f660dc3abb0eeea6583caff0b6c3d89acd553d3ecfe87d1ffcdedec7f788c59a2b060"},
{"id":"a1e75e26add87d78cbdf5d1f67171b54a06b4d77b0a51d11beb44a5b1d17c1d8","pubkey":"2872ab118f7acd64cab3982a7235b9c633b5fffd3996f8dfc0e96c7172e164a7","created_at":1722862764,"kind":1,"tags":[["proxy","https://mstdn.jp/@aimiteno_c/112909534102708984","web"],["t","男子バレー"],["proxy","https://mstdn.jp/users/aimiteno_c/statuses/112909534102708984","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://mstdn.jp/users/aimiteno_c/statuses/112909534102708984","pink.momostr"],["-"]],"content":"めちゃくちゃええ試合や！\n #男子バレー","sig":"d1a5e03fec036075e9e26dc087c3c4a7f6dd601855353d62365405d5996b1164612e2bbf2e9612083757e900801dad691a5eacd9c95f1490a4043ea8b66a3b10"},
{"id":"d0b14c0f54ef7d89e866ec97bdab76ff3fdfddf710e38059b72695dee9ceff9f","pubkey":"26130449c250be4ad281889e399d58825275ed3347b7ff8e7590a19894170485","created_at":1722861995,"kind":1,"tags":[["e","ef9673b6dfcd4c7ce61971c5d2276da5ad1999892a89d0331911f1b1d3829eb4","","reply","26130449c250be4ad281889e399d58825275ed3347b7ff8e7590a19894170485"],["p","26130449c250be4ad281889e399d58825275ed3347b7ff8e7590a19894170485"],["e","db79b7952560cad1ac01b657d349e279b9d4a54d13c0e6deeb7b7a2296e6568e","","root"],["content-warning","6話"],["proxy","https://misskey.io/notes/9wkjv18foeum0426","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://misskey.io/notes/9wkjv18foeum0426","pink.momostr"],["-"]],"content":"ピースじゃんけん：チョキ\nポップ・ジョーカー登場回。ワンシーンワンシーンの絵作りが面白くて見てて飽きない。スマイルの質の良さが存分に感じられる。","sig":"996b815e3ce9bcbb89e4e951c51ce854606bfa169dc13282e707b6d046985bc06de4c373f39fe6cb8cf5f261b459ad24cb00257f1e3c25b5f77b642c0eb9f68d"},
{"id":"c2dceb7d30fbd63867294e479fcae0886f3ff9cfa19fae4fbf47c4d275f31689","pubkey":"8559b2440a5b774f226fe28df8ea37e4c55eb8f98412c85802a5d06ae6675b22","created_at":1722861901,"kind":1,"tags":[["t","barbadoslife"],["t","barbadosphotography"],["t","cranebeach"],["imeta","url https://files.mastodon.social/media_attachments/files/112/909/477/578/392/383/original/14ada0380ebe91cd.jpeg","m image/jpeg"],["t","travel"],["t","caribbean"],["t","barbados"],["t","barbadostravel"],["t","cityscape"],["t","adventure"],["t","barbadostourism"],["t","explore"],["t","barbadosvacation"],["t","barbadosexplore"],["t","cranebeachlife"],["t","barbadosnature"],["proxy","https://mastodon.social/@travolax/112909477606817363","web"],["t","barbadosdesign"],["t","cranebeachbarbados"],["t","outdoors"],["t","VisitBarbados"],["t","nature"],["t","sea"],["t","beach"],["t","caribbeansea"],["t","barbadoslove"],["t","beachlife"],["proxy","https://mastodon.social/users/travolax/statuses/112909477606817363","activitypub"],["L","pink.momostr"],["l","pink.momostr.activitypub:https://mastodon.social/users/travolax/statuses/112909477606817363","pink.momostr"],["-"]],"content":"Crane Beach, Barbados 🇧🇧\n\n#cranebeach #barbados #travel #explore #adventure #nature #beach #barbadostravel #visitbarbados #barbadoslove #barbadosexplore #barbadosphotography #barbadosvacation #barbadostourism #caribbean #cityscape #barbadosdesign #barbadosnature #cranebeachbarbados #outdoors #sea #caribbeansea #beachlife #barbadoslife #cranebeachlife\nhttps://files.mastodon.social/media_attachments/files/112/909/477/578/392/383/original/14ada0380ebe91cd.jpeg\n","sig":"89785a4c9bc2160366f5901a93f75fd7cc14db5124a7039a3d3951a260202be521d021965c8afd8d0e98db9cb615dedd9ea83560a19c39b12c1bafa0554bb001"},
{"id":"ac33b8b86c6f778dfe924b53c4bb4cf035f459ce069c4dec57fe49416a165622","pubkey":"266815e0c9210dfa324c6cba3573b14bee49da4209a9456f9484e5106cd408a5","created_at":1722861866,"kind":1,"tags":[["e","5060556f8a031e53492d5d43fca05e905569a8d6de7207c600bfb302a5d07149","","mention"],["e","71216eeaaedb546413a1504d0f1bb6c411955e90ef223c10c38944577616c56a","","root"],["e","71216eeaaedb546413a1504d0f1bb6c411955e90ef223c10c38944577616c56a","","reply"],["p","5c508c34f58866ec7341aaf10cc1af52e9232bb9f859c8103ca5ecf2aa93bf78","","mention"],["p","803a613997a26e8714116f99aa1f98e8589cb6116e1aaa1fc9c389984fcd9bb8","","mention"]],"content":"I've been using this\nnostr:nevent1qvzqqqqqqypzpqp6vyue0gnwsu2pzmue4g0e36zcnjmpzms64g0unsufnp8umxacqqs9qcz4d79qx8jnfyk46slu5p0fq4tf4rtduus8ccqtlvcz5hg8zjgkh6y3a","sig":"ce41eec7329fc774f630f0df3af3f1c78d292b8587767b0de5dff8a550ab986461fc1284f43ac365c2835cafb39111ddf5c026c0169ba442ee64ee4ecc4547a2"}
    ]"#;
    let es: Vec<Event> = serde_json::from_str(e).unwrap();
    c.bench_function("matches", |b| {
        b.iter(|| {
            let n = es.iter().filter(|e| f.matches(e)).count();
            assert_eq!(n, 20);
        })
    });
}

criterion_group!(benches, matches);
criterion_main!(benches);
