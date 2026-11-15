$(function () {
    var host = location.protocol + '//'; // http://
    if (isIPV6) {
        if (portal.AuthIP6 != '') {
            portal.AuthIP6 = '[' + portal.AuthIP6 + ']';
        }
        host += portal.AuthIP6 || location.hostname;
    } else {
        host += portal.AuthIP || location.hostname;
    }

    /**
     * 设置 Cookie
     * @name            {String}    Cookie 名称
     * @value           {String}    Cookie 值
     * @day             {Number}    有效期
     */
    var setCookie = function (name, value, day) {
        if (day === undefined) day = 7;
        var date = new Date();
        date.setDate(date.getDate() + day);
        document.cookie = name + '=' + value + ';expires=' + date;
    };

    /**
     * 读取 Cookie
     * @name            {String}    Cookie 名称
     * @return          {String}    Cookie 值
     */
    var getCookie = function (name) {
        var reg = RegExp(name + '=([^;]+)');
        var arr = document.cookie.match(reg);
        return arr ? arr[1] : false;
    };

    /**
     * 删除 Cookie
     * @name            {String}    Cookie 名称
     */
    var delCookie = function (name) {
        setCookie(name, '', -1);
    };

    // 记住 域
    (function () {
        var cookie = {
            domain: getCookie('domain'),
        };
        var hasDomain = $('#domain').val() !== undefined;
        if (hasDomain && cookie.domain) {
            $('#domain').val(cookie.domain);
        }
        $('#login').click(function () {
            if (hasDomain) {
                var domain = $('#domain').val();
                setCookie('domain', domain);
            }
        });
    })();

    //自服务地址
    var selfServiceIp = location.protocol + '//' + location.hostname + ':8800';
    if (portal.ServiceIP != '') {
        selfServiceIp = portal.ServiceIP;
    }

    /**
     * Show Error Message
     * @obj.success     {Boolean}   消息类型    Default:false
     * @obj.message     {String}    消息内容
     * @obj.back        {Boolean}   返回上页    Default:false
     * @obj.callback    {Function}  确认回调
     */
    function showErrorMessage(obj) {
        var success = obj.success;
        var back = obj.back;
        var message = translate[obj.message] || obj.message;
        var callback = obj.callback;

        layer.alert(message, {
            icon: success ? 1 : 2,
            skin: 'layui-layer-molv',
            btn: [(translate['OK'] || 'OK')],
            title: translate['Info'] || 'Info'
        }, function (index) {
            if (back) window.history.go(-1);
            if (callback) callback();
            layer.close(index);
        });
    }

    function getQueryString(name) {
        var reg = new RegExp('(^|&)' + name + '=([^&]*)(&|$)', 'i');
        var r = window.location.search.substr(1).match(reg);
        if (r != null) return unescape(r[2]);
        return '';
    }

    //show log
    function showLog(username) {
        $.Log(host, {username: username}, function (data) {
            var message = data.Message;
            var error = 'NoResponseDataError';
            if (data.Message != '') {
                if (data.Message.indexOf('E') == 0) {
                    error = data.Message.substr(0, 5);
                } else {
                    error = data.Message;
                }
            }
            if (error != 'E2901' && typeof (translate[error]) != 'undefined') {
                message = translate[error];
            }
            layer.alert(message, {
                icon: 2,
                skin: 'layui-layer-molv',
                btn: [(translate['OK'] || 'OK')],
                title: translate['Info'] || 'Info'
            });
        });
    }

    //if login to success page
    if (typeof success == 'undefined' || !success) {
        if ((typeof wechat == 'undefined' || !wechat) && (typeof msg == 'undefined' || !msg)) {
            var wait = getQueryString('srun_wait');
            if (wait != '1') {
                $.Info(host, {}, function (data) {
                    if (data.error == 'ok') {
                        location.href = './srun_portal_success' + location.search;
                    }
                });
            }
        }
    }

    //self-service
    $('#self-service').click(function () {
        var ALPHA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/',
            base64 = $.base64;
        base64.setAlpha(ALPHA);
        var data = base64.encode(lang);

        var username = $('#username').val(),
            password = $('#password').val();
        if (typeof username != 'undefined' && username != '') {
            if (typeof password != 'undefined' && password != '') {
                var pwd = md5(password);
                data = base64.encode(lang + ':' + username + ':' + pwd);
            }
            if (typeof success != 'undefined' && success) {
                data = base64.encode(lang + ':' + username + ':' + username);
            }
        }
        window.open(selfServiceIp + '/site/sso?data=' + data);
        return;
    });

    //Change Language
    $('#language').click(function () {
        var language = $(this).text();
        var l = 'zh-CN';
        if (typeof language != 'undefined' && language == 'English') {
            l = 'en-US';
        } else {
            if (typeof lang != 'undefined') {
                l = lang == 'zh-CN' ? 'en-US' : 'zh-CN';
            }
        }
        $.Language(l);
    });

    var messageUri = '/v2/srun_portal_message';
    if (portal.MsgApi == 'old') {
        messageUri = '/v1/srun_portal_message';
    }
    //Message
    $.Message(host, messageUri, {}, function (data) {
        var viewType = $('#viewType').val();
        if (data.Code == 0 && data.Data != null) {
            if (data.Data.length > 0) {
                if (portal.MsgApi == 'old') {
                    $('#notice-title').text(data.Data[0].msg_head);
                    $('#notice-content').html(data.Data[0].msg_con);
                    return;
                }

                if (viewType === 'one' || typeof viewType == 'undefined') {
                    $('#notice-title').text(data.Data[0].Title);
                    $('#notice-content').html(data.Data[0].Content);
                } else if (viewType === 'list') {
                    $('.notice-heading')
                        .after('<div class=\'notice-list\'><div class=\'notice-content\'></div></div>');
                    //data.Data = data.Data.reverse();
                    var href = selfServiceIp + '/news/view?id=';
                    var pageNum = Math.ceil(data.Data.length / 5);
                    var lastPage = data.Data.length % 5;
                    var pageContent = new Array();
                    var li = '';
                    for (var i = 0; i < pageNum; i++) {
                        // 控制器按钮
                        li += '<li>' + (i + 1) + '</li>';
                        // 末页内容
                        if ((i + 1) == pageNum) {
                            pageContent[i] = data.Data.slice(i * 5, i * 5 + lastPage);
                        }
                        // 内容
                        pageContent[i] = data.Data.slice(i * 5, i * 5 + 5);
                    }
                    var control = '<div class=\'control\'><ul>' + li + '</ul></div>';
                    $('.notice-list').append(control);
                    var listEcho = function (n) {
                        var content = '';
                        for (var i = 0; i < pageContent[n].length; i++) {
                            content += '<tr><td>' + (i + 1 + n * 5) + '</td><td><a href=\'' + href + pageContent[n][i].Id + '\' target=\'_blank\'>' + pageContent[n][i].Title + '</a></td><td>' + getMyDate((pageContent[n][i].Created_at) * 1000) + '</td></tr>';
                        }
                        var table = '<table class=\'table table-hover\'><thead><tr ><th>#</th><th>通知标题</th><th>通知时间</th></tr></thead><tbody>' + content + '</tbody></table>';
                        $('.notice-list .notice-content').html(table);
                    };
                    listEcho(0);
                    $('.notice-list .control li:eq(0)').addClass('active');
                    $(document).on('click', '.notice-list .control li', function () {
                        $('.notice-list .control li').removeClass('active');
                        $(this).addClass('active');
                        listEcho($(this).index());
                    });
                    $('.notice-heading').attr('style', 'margin:25px 0 0 10px;');
                }
            }
        }
    });

    //时间戳转换成时间格式
    function getMyDate(str) {
        var oDate = new Date(str),
            oYear = oDate.getFullYear(),
            oMonth = oDate.getMonth() + 1,
            oDay = oDate.getDate(),
            oHour = oDate.getHours(),
            oMin = oDate.getMinutes(),
            oSen = oDate.getSeconds(),
            oTime = oYear + '-' + getzf(oMonth) + '-' + getzf(oDay) + ' ' + getzf(oHour) + ':' + getzf(oMin) + ':' + getzf(oSen);//最后拼接时间
        return oTime;
    }

    //补0操作
    function getzf(num) {
        if (parseInt(num) < 10) {
            num = '0' + num;
        }
        return num;
    }

    function IpAlreadyRetryAuth(params) {
        var logoutParams = {
            ac_id: params.acid,
            ip: params.ip,
            username: params.username
        };
        //采用DM下线，避免有使用Mac无感知用户采用普通下线下不了问题
        $.DM(host, logoutParams, function (data) {
            if (data.error === 'ok') {
                $.Login(host, params, function (resp) {
                    if (resp.error === 'ok') {
                        location.href = './srun_portal_success' + location.search;
                        return;
                    }
                    showErrorMessage({
                        message: resp.message
                    });
                });
            } else {
                //Show Error Message
                showErrorMessage({
                    message: data.message
                });
            }
        });
    }

    //回车完成
    $(document).keydown(function (e) {
        if (e.keyCode === 13) {
            $('#login').click().addClass('active');
            setTimeout(function () {
                $('#login').removeClass('active');
            }, 100);
        }
    });

    //Login
    $('#login').click(function () {
        //username is empty
        var username = $('#username').val();
        if (username == '') {
            $('#username').focus();
            return;
        }
        //password is empty
        var password = $('#password').val();
        if (password == '') {
            $('#password').focus();
            return;
        }
        var acid = $('#ac_id').val(),
            ip = $('#user_ip').val();

        var otp = $('#otp').val();
        if (typeof otp == 'undefined') {
            otp = false;
        } else {
            otp = true;
        }
        var params = {
            //username:$.trim(username).toLowerCase(),
            username: $.trim(username),
            domain: '',
            password: password,
            ac_id: acid,
            ip: ip,
            double_stack: 0,
            otp: otp
        };
        if ($('#domain').val() != undefined) {
            params.domain = $('#domain').val();
        }
        var ua = window.navigator.userAgent;
        var md = new MobileDetect(ua);
        var mobile = md.mobile();
        if ((!mobile && portal.DoubleStackPC) || (mobile && portal.DoubleStackMobile)) {
            params.double_stack = 1;
        }

        $.Login(host, params, function (data) {
            if (data.error === 'ok') {
                //登录成功后，解除 srun_wait 限制
                var uriParams = location.search;
                var wait = getQueryString('srun_wait');
                if (wait === '1') {
                    uriParams = uriParams.replace(/srun_wait=1&/, '');
                }

                //判断是否符合双栈认证
                if ((!mobile && portal.DoubleStackPC) || (mobile && portal.DoubleStackMobile)) {
                    var doubleHost = location.protocol + '//';
                    //判断当前Portal IP是否是IPv6，如果是则向v4的地址上进行双栈认证
                    if (isIPV6) {
                        //双栈地址是IPv4
                        doubleHost += portal.AuthIP || location.hostname;
                    } else {
                        //IPv6认证地址
                        if (portal.AuthIP6 != '') {
                            portal.AuthIP6 = '[' + portal.AuthIP6 + ']';
                        }
                        doubleHost += portal.AuthIP6 || location.hostname;
                    }

                    params.double_stack = 0;
                    params.ip = '';
                    params.password = password;
                    $.Login(doubleHost, params, function (data) {
                    });

                    setTimeout(function () {
                        location.href = './srun_portal_success' + uriParams + '&srun_domain=' + params.domain;
                    }, 500);
                } else {
                    if (data.message != '') {
                        showErrorMessage({
                            message: data.message
                        });
                    }

                    //重定向到成功页
                    setTimeout(function () {
                        location.href = './srun_portal_success' + uriParams + '&srun_domain=' + params.domain;
                    }, 1000);
                }
            } else {
                //先下线在上线
                if (data.message === 'IpAlreadyOnlineError') {
                    params.password = password;
                    IpAlreadyRetryAuth(params);
                } else {
                    if (data.message === 'NoResponseDataError') {
                        showLog(username);
                        return;
                    }
                    //Show Error Message
                    if (data.message === 'NotOnlineError') {
                        setTimeout(function () {
                            location.href = './srun_portal_success' + location.search;
                        }, 1500);
                    } else {
                        showErrorMessage({
                            message: data.message
                        });
                    }
                }
            }
        });
    });

    //Logout DM
    $('#logout-dm').click(function () {
        //username is empty
        var username = $('#username').val();
        if (username == '') {
            $('#username').focus();
            return;
        }
        var params = {
            username: username,
            domain: '',
            ac_id: $('#ac_id').val(),
            ip: $('#user_ip').val()
        };
        if ($('#domain').val() != undefined) {
            params.domain = $('#domain').val();
        }
        $.DM(host, params, function (data) {
            if (data.error == 'ok') {
                //Show DM Logout OK!
                location.href = './srun_portal_pc' + location.search + '&srun_domain=' + params.domain;
            } else {
                if (data.message == 'NoResponseDataError') {
                    showLog(username);
                    return;
                }
                //Show Error Message
                showErrorMessage({
                    message: data.message
                });
            }
        });
    });

    //Logout
    $('#logout').click(function () {
        var acid = $('#ac_id').val(),
            username = $('#username').val();
        var params = {
            username: username,
            ac_id: acid,
            ip: $('#user_ip').val(),
            domain: $('#domain').val()
        };
        var ua = window.navigator.userAgent;
        var md = new MobileDetect(ua);
        var mobile = md.mobile();
        $.Logout(host, params, function (data) {
            if (data.error == 'ok') {
                isLogin = false;
                //判断是否符合双栈认证
                if ((!mobile && portal.DoubleStackPC) || (mobile && portal.DoubleStackMobile)) {
                    var doubleHost = location.protocol + '//';
                    if (isIPV6) {
                        doubleHost += portal.AuthIP || location.hostname;
                    } else {
                        if (portal.AuthIP6 != '') {
                            portal.AuthIP6 = '[' + portal.AuthIP6 + ']';
                        }
                        doubleHost += portal.AuthIP6 || location.hostname;
                    }

                    params.double_stack = 0;
                    params.ip = '';
                    $.Logout(doubleHost, params, function (data) {
                        if (data.error == 'ok') {
                            //Redirect Login Page
                            location.href = './index_' + acid + '.html?srun_wait=1';
                        }
                    });
                } else {
                    //Redirect Login Page
                    location.href = './index_' + acid + '.html?srun_wait=1';
                }
            } else {
                if (data.message == 'NoResponseDataError') {
                    showLog(username);
                    return;
                }
                //Show Error Message
                showErrorMessage({
                    message: data.message,
                    back: true
                });
            }
        });
    });

    //Get Verify Code
    //clearTimeout(loading);//停止倒计时
    $('#code').click(function () {
        $this = $(this);
        if ($this.hasClass('disabled')) {
            return false;
        }
        //phone is phone?
        var phone = $('#username').val();
        if (phone == '') {
            $('#username').focus();
            return;
        }
        var ip = $('#user_ip').val();
        var mac = $('#user_mac').val();
        var wait = 60,
            loading;

        function time(t) {
            if (t == 1) {
                $this.text(translate['GetVerifyCode'] || '获取验证码');
                $this.removeClass('disabled');
            } else {
                t--;
                $this.text(t + (translate['S'] || '秒'));
                loading = setTimeout(function () {
                    time(t);
                }, 1000);
            }
        }

        $this.addClass('disabled');
        time(wait);
        var params = {
            phone: phone,
            ip: ip,
            mac: mac
        };
        $.GetVerifyCode(host, params, function (data) {
            if (data.error == 'ok') {
                //Show Success Message
                showErrorMessage({
                    success: true,
                    message: 'SendVerifyCodeOK'
                });
            } else {
                //Stop
                clearTimeout(loading);
                $this.text(translate['GetVerifyCode'] || '获取验证码');
                $this.removeClass('disabled');
                //Show Error Message
                showErrorMessage({
                    message: data.message
                });
            }
        });
    });

    //SMS Auth
    $('#sms-login').click(function () {
        //phone is phone?
        var phone = $('#username').val();
        if (phone == '') {
            $('#username').focus();
            return;
        }
        //vcode is empty
        var vcode = $('#vcode').val();
        if (vcode == '') {
            $('#vcode').focus();
            return;
        }
        var ip = $('#user_ip').val();
        var mac = $('#user_mac').val();
        var ac_id = $('#ac_id').val();
        var params = {
            phone: phone,
            ip: ip,
            mac: mac,
            vcode: vcode,
            ac_id: ac_id
        };
        $.SmsAuth(host, params, function (data) {
            if (data.error == 'ok') {
                //Redirect Success Page
                location.href = './srun_portal_success' + location.search;
            } else {
                if (data.message == 'NoResponseDataError') {
                    showLog('smpv_' + phone);
                    return;
                }
                //Show Error Message
                showErrorMessage({
                    message: data.message
                });
            }
        });
    });

    $('#ddi-double-auth').click(function () {
        $.DdiDoubleAuth(host, function (data) {
            if (data.code != 0) {
                alert(data.message);
                return;
            }
            alert(data.message);
        });
    });

    //Success Page
    if (typeof success != 'undefined' && success) {
        $.Info(host, {}, function (data) {
            if (data.error == 'ok') {
                $.Detect(host, function (response) {
                    if (response.Redirect) {
                        if ($.isMobile() && response.Mobile != '') {
                            window.open(response.Mobile);
                        } else if (response.Pc != '') {
                            window.open(response.Pc);
                        }
                    }
                });
                $('#username').val(data.user_name);
                $('#user_name').html(data.user_name);
                $('#used_flow').html(data.used_flow);
                $('#used_time').html(data.used_time);
                $('#balance').html(data.balance);
                $('#ip').html(data.ip);
                var domain = getQueryString('srun_domain');
                if (domain != '' && data.domain != '' && data.user_name.indexOf(data.domain) == -1) {
                    $('#domain').val('@' + data.domain);
                }
            } else {
                //show error message
                showErrorMessage({
                    message: data.message,
                    back: true,
                });
            }
        });
        if ($('#visitor-qrcode').length > 0) {
            $('#visitor-qrcode').click(function () {
                var params = {
                    uri: '/api/v1/user/token-visitors',
                    user_name: $('#username').val()
                };
                $.PortalProxy(host, params, function (response) {
                    if (response.error == 'ok') {
                        layer.open({
                            title: '',
                            btn: [],
                            content: $('#formbox').html(),
                            shade: 0.7,
                            shadeClose: true
                        });
                        $('#layer-qrcode').qrcode({
                            text: JSON.stringify(response.data),
                            height: 150,
                            width: 150,
                            background: '#ffffff',
                            foreground: '#4086CE'
                        });
                    } else {
                        layer.alert(response.message, {
                            icon: 2, skin: 'layui-layer-molv', btn: '确定', title: '信息'
                        });
                    }
                });
            });
        }
    }

    //Qrcode Page
    if (typeof qrcode != 'undefined' && qrcode) {
        var username = $('#username').val();
        var params = {
            key: $('#key').val(),
            username: username,
            ac_id: $('#ac_id').val()
        };
        $.Qrcode(host, params, function (data) {
            if (data.error == 'ok') {
                //Redirect Success Page
                location.href = './srun_portal_success?ac_id=' + params.ac_id + '&theme=' + theme;
            } else {
                //Show Error Message
                if (data.message == 'NoResponseDataError') {
                    showLog(username);
                    return;
                }
                showErrorMessage({
                    message: data.message
                });
            }
        });
    }

    //Wechat
    if (typeof wechat != 'undefined' && wechat) {
        var ua = navigator.userAgent;
        var isIOS = false;
        if (ua.indexOf('iPhone') != -1 || ua.indexOf('iPod') != -1 || ua.indexOf('iPad') != -1) {   //iPhone|iPod|iPad
            isIOS = true;
        }
        var wifiConfig = {
            mac: getQueryString('rmac'),
            bssid: getQueryString('bssid'),
            token: getQueryString('sruntoken')
        };
        if (isIOS && location.search.indexOf('sruntoken') == -1) { //now request provisional release
            var params = {ac_id: acid, ip: ip};
            $.Release(host, params, function (data) {
                if (data.error == 'ok') {
                    $('#call').text(translate['Wait'] || '请等待...');
                    $('#call').addClass('disabled');
                    wifiConfig = data.wifiConfig;
                    location.href = './srun_portal_weixin' + location.search + '&rmac=' + wifiConfig.mac + '&bssid=' + wifiConfig.bssid + '&sruntoken=' + wifiConfig.token;
                    return;
                }
                //show error message
                showErrorMessage({
                    message: data.message
                });
                //disabled
                $('#call').addClass('disabled');
            });
        }
        $('#call').click(function () {
            if ($(this).hasClass('disabled')) {
                showErrorMessage({
                    message: 'IsEvokingWeChat',
                });
                return false;
            }
            $('#call').addClass('disabled');
            if (!isIOS) {//no ios,request provisional release and auth
                var params = {ac_id: acid, ip: ip, ssid: ssid};
                $.WeixinRelease(host, params, function (data) {
                    if (data.error == 'ok') {
                        config = data.wifiConfig;
                        Wechat_GotoRedirect(
                            config.appid,
                            config.extend,
                            config.timestamp,
                            config.sign,
                            config.shop_id,
                            config.authUrl,
                            config.mac,
                            config.ssid,
                            config.bssid
                        );
                        return;
                    }
                    //show error message
                    showErrorMessage({
                        message: data.message
                    });
                });
            } else {
                //weixin auth
                var params = {
                    ac_id: acid,
                    ip: ip,
                    ssid: ssid,
                    bssid: wifiConfig.bssid || '',
                    mac: wifiConfig.mac || '',
                    token: wifiConfig.token || ''
                };
                $.WeiXinCall(host, params, function (data) {
                    if (data.error == 'ok') {
                        config = data.wifiConfig;
                        Wechat_GotoRedirect(
                            config.appid,
                            config.extend,
                            config.timestamp,
                            config.sign,
                            config.shop_id,
                            config.authUrl,
                            config.mac,
                            config.ssid,
                            config.bssid
                        );
                        return;
                    }
                    //show error message
                    showErrorMessage({
                        message: data.message
                    });
                });
            }
        });
    }

    //SSO
    if (typeof sso != 'undefined' && sso) {
        $.SSO(host, {}, function (data) {
            if (data.error === 'ok') {
                //Redirect success page
                location.href = './srun_portal_success?ac_id=' + data.ac_id;
            } else {
                if (data.code === 301) {
                    location.href = data.redirect;
                } else if (data.code === 200) {
                    return true;
                } else {
                    //Show Error Message
                    /*showErrorMessage({
                        message: data.message
                    });*/
                    // 出错后，将错误写到页面上，取消loading效果
                    $(".loading").removeClass("loading").addClass("loading-end").html(translate[data.message]);
                }
            }
        });
    }

    //统一认证入口 oauth协议认证入口
    $('#ssoLogin').click(function () {
        var ssoError = getQueryString('ssoError');
        if (ssoError == '1') {
            alert('请使用本地认证服务登陆，若需访问单点登陆，删除地址栏中 &ssoError=1 并刷新浏览器');
        } else {
            // oauth 主题下的 cas 设置的是 oauth = true,所以与sso不影响
            return location.href = '/srun_portal_sso' + location.search;
        }
    });

    //oauth 协议认证
    if (typeof oauth !== 'undefined' && oauth) {
        //针对映射地址来处理，如果浏览器的主机是映射地址， 认证成功后下线提示设备不在线。需要将
        if (location.host != portal.AuthIP) {
            location.href = location.protocol + '//' + portal.AuthIP + '/srun_portal_sso' + location.search;
        }

        // 为单点登陆增加超时判断 (10s)
        var timeout = true;
        setTimeout(function () {
            if (timeout) {
                showErrorMessage({
                    message: 'SsoServerError',
                    callback: function () {
                        location.href = './srun_portal_pc' + location.search;
                    }
                });
            }
        }, 10000);

        $.OAuth(host, {}, function (data) {
            if (data.error === 200) {
                //Redirect success page
                location.href = '/srun_portal_success?ac_id=' + data.ac_id;
            } else {
                // 301 重定向
                if (data.code === 301) location.href = data.redirect;

                // 501 错误，弹框确认回调为返回登录页
                if (data.code === 501) {
                    showErrorMessage({
                        message: data.message,
                        callback: function () {
                            location.href = host;
                        }
                    });
                }

                // 502 域名校验不合法，弹框确认回调为返回登录页，并禁止再次单点登陆
                if (data.code === 502) {
                    showErrorMessage({
                        message: data.message,
                        callback: function () {
                            location.href = host + '&ssoError=1';
                        }
                    });
                }
            }
        });
    }

    function utf16to8(str) {
        var out,
            i,
            len,
            c;
        out = '';
        len = str.length;
        for (i = 0; i < len; i++) {
            c = str.charCodeAt(i);
            if ((c >= 0x0001) && (c <= 0x007F)) {
                out += str.charAt(i);
            } else if (c > 0x07FF) {
                out += String.fromCharCode(0xE0 | ((c >> 12) & 0x0F));
                out += String.fromCharCode(0x80 | ((c >> 6) & 0x3F));
                out += String.fromCharCode(0x80 | ((c >> 0) & 0x3F));
            } else {
                out += String.fromCharCode(0xC0 | ((c >> 6) & 0x1F));
                out += String.fromCharCode(0x80 | ((c >> 0) & 0x3F));
            }
        }
        return out;
    }

    //Qrcode
    var socket = false;
    $('.login-table a').on('click', function () {
        if ($(this).hasClass('checked') !== true) {
            $('.login-table a').removeClass('checked');
            $(this).addClass('checked');
        }
        if ($(this).parent().hasClass('login-table-l') !== true) {
            $('#out-qrcode').html('');
            $('#login-form').addClass('hidden');
            $('#out-qrcode').removeClass('hidden');
            var info = {
                'ac_id': $('#ac_id').val(),
                'ip': $('#user_ip').val()
            };
            var ALPHA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/',
                base64 = $.base64;
            base64.setAlpha(ALPHA);
            i = base64.encode(JSON.stringify(info));
            var params = {
                'Tips': utf16to8('请在\'迅连\'小程序中扫码认证'),
                'info': i
            };
            var text = JSON.stringify(params);
            var $li_1 = $('<div id=\'out-qrcode1\'class=\'col-xs-12 col-md-6\' style=\'float: left;\'><div class=\'out-qrcode1\'></div><p class=\'text-center\'style="font-size:16px;">第二步:小程序中认证后<br/>点击右上角扫这里</p></div>');
            var $parent = $('#out-qrcode');
            $parent.append($li_1);
            $('.out-qrcode1').qrcode({
                text: text,
                height: 150,
                width: 150,
                src: '/static/images/basic/qrcode-logo.png',
                background: '#ffffff', //背景颜色
                foreground: '#4086CE' //前景颜色
            });
            $('#out-qrcode')
                .prepend('<div class="col-xs-12 col-md-6" style=" float: left; text-align: center;"><img src="/static/images/basic/small-app-logo.jpg" style="width:150px;height:150px;vertical-align:inherit;"/><p class=\'text-center\' style="font-size:16px;">第一步:扫这里<br/>进入迅连小程序上网</p></div>');

            // 连接socket
            var uid = $('#user_ip').val() + $('#ac_id').val();
            socket = io('http://' + document.domain + ':2120');
            socket.on('connect', function () {
                socket.emit('login', uid);
            });
            socket.on('new_msg', function (msg) {
                console.log(msg);
            });
            socket.on('update_online_count', function (online_stat) {
                console.log(online_stat);
            });
            // 扫码登录成功时
            socket.on('qrcode_auth_success', function (data) {
                console.log(data);
                setTimeout(function () {
                    location.reload();
                }, 1000);
            });
        } else {
            // 关闭socket
            if (socket !== false) {
                socket.close();
            }
            location.reload();
        }
    });
	$('#forget').click(function () {
        window.open('http://202.198.1.80:8800/forget/phone')
    })
});
