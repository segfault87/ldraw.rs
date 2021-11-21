// Portions of shader code has been taken from Three.js source code.
//
// Copyright © 2010-2021 three.js authors

precision mediump float;
precision highp float;
precision highp int;

in vec4 vColor;

out vec4 fragColor;

#ifdef WITHOUT_BFC
    void main() {
        fragColor = vColor;
    }
#else
    in vec3 vNormal;
    in vec3 vViewPosition;

    uniform vec3 diffuse;
    uniform vec3 emissive;
    uniform float roughness;
    uniform float metalness;
    uniform float opacity;

    uniform sampler2D envMap;

    uniform mat4 viewMatrix;
    uniform bool isOrthographic;

    #ifndef saturate
        #define saturate( a ) clamp( a, 0.0, 1.0 )
    #endif

    vec4 LinearTosRGB( in vec4 value ) {
        return vec4( mix( pow( value.rgb, vec3( 0.41666 ) ) * 1.055 - vec3( 0.055 ), value.rgb * 12.92, vec3( lessThanEqual( value.rgb, vec3( 0.0031308 ) ) ) ), value.a );
    }

    vec4 LinearToLinear( in vec4 value ) {
        return value;
    }
    
    vec4 linearToOutputTexel( vec4 value ) {
        return LinearToLinear( value );
    }

    vec4 RGBEToLinear( in vec4 value ) {
        return vec4( value.rgb * exp2( value.a * 255.0 - 128.0 ), 1.0 );
    }

    vec4 envMapTexelToLinear( vec4 value ) {
        return RGBEToLinear( value );
    }

    #define PI 3.141592653589793
    #define PI2 6.283185307179586
    #define PI_HALF 1.5707963267948966
    #define RECIPROCAL_PI 0.3183098861837907
    #define RECIPROCAL_PI2 0.15915494309189535
    #define EPSILON 1e-6

    #ifndef saturate
        #define saturate( a ) clamp( a, 0.0, 1.0 )
    #endif

    #define whiteComplement( a ) ( 1.0 - saturate( a ) )

    float pow2( const in float x ) {
        return x*x;
    }
    float pow3( const in float x ) {
        return x*x*x;
    }
    float pow4( const in float x ) {
        float x2 = x*x;
        return x2*x2;
    }
    float max3( const in vec3 v ) {
        return max( max( v.x, v.y ), v.z );
    }
    float average( const in vec3 color ) {
        return dot( color, vec3( 0.3333 ) );
    }
    highp float rand( const in vec2 uv ) {
        const highp float a = 12.9898, b = 78.233, c = 43758.5453;
        highp float dt = dot( uv.xy, vec2( a, b ) ), sn = mod( dt, PI );
        return fract( sin( sn ) * c );
    }
    float precisionSafeLength( vec3 v ) {
        return length( v );
    }
    struct IncidentLight {
        vec3 color;
        vec3 direction;
        bool visible;
    };
    struct ReflectedLight {
        vec3 directDiffuse;
        vec3 directSpecular;
        vec3 indirectDiffuse;
        vec3 indirectSpecular;
    };
    struct GeometricContext {
        vec3 position;
        vec3 normal;
        vec3 viewDir;
    };
    struct PhysicalMaterial {
        vec3 diffuseColor;
        float roughness;
        vec3 specularColor;
        float specularF90;
    };
    vec3 transformDirection( in vec3 dir, in mat4 matrix ) {
        return normalize( ( matrix * vec4( dir, 0.0 ) ).xyz );
    }
    vec3 inverseTransformDirection( in vec3 dir, in mat4 matrix ) {
        return normalize( ( vec4( dir, 0.0 ) * matrix ).xyz );
    }
    mat3 transposeMat3( const in mat3 m ) {
        mat3 tmp;
        tmp[ 0 ] = vec3( m[ 0 ].x, m[ 1 ].x, m[ 2 ].x );
        tmp[ 1 ] = vec3( m[ 0 ].y, m[ 1 ].y, m[ 2 ].y );
        tmp[ 2 ] = vec3( m[ 0 ].z, m[ 1 ].z, m[ 2 ].z );
        return tmp;
    }

    const float PackUpscale = 256. / 255.;
    const float UnpackDownscale = 255. / 256.;
    const vec3 PackFactors = vec3( 256. * 256. * 256., 256. * 256., 256. );
    const vec4 UnpackFactors = UnpackDownscale / vec4( PackFactors, 1. );
    const float ShiftRight8 = 1. / 256.;

    vec3 BRDF_Lambert( const in vec3 diffuseColor ) {
        return RECIPROCAL_PI * diffuseColor;
    }
    vec3 F_Schlick( const in vec3 f0, const in float f90, const in float dotVH ) {
        float fresnel = exp2( ( - 5.55473 * dotVH - 6.98316 ) * dotVH );
        return f0 * ( 1.0 - fresnel ) + ( f90 * fresnel );
    }
    float V_GGX_SmithCorrelated( const in float alpha, const in float dotNL, const in float dotNV ) {
        float a2 = pow2( alpha );
        float gv = dotNL * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNV ) );
        float gl = dotNV * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNL ) );
        return 0.5 / max( gv + gl, EPSILON );
    }
    float D_GGX( const in float alpha, const in float dotNH ) {
        float a2 = pow2( alpha );
        float denom = pow2( dotNH ) * ( a2 - 1.0 ) + 1.0;
        return RECIPROCAL_PI * a2 / pow2( denom );
    }
    vec3 BRDF_GGX( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in vec3 f0, const in float f90, const in float roughness ) {
        float alpha = pow2( roughness );
        vec3 halfDir = normalize( lightDir + viewDir );
        float dotNL = saturate( dot( normal, lightDir ) );
        float dotNV = saturate( dot( normal, viewDir ) );
        float dotNH = saturate( dot( normal, halfDir ) );
        float dotVH = saturate( dot( viewDir, halfDir ) );
        vec3 F = F_Schlick( f0, f90, dotVH );
        float V = V_GGX_SmithCorrelated( alpha, dotNL, dotNV );
        float D = D_GGX( alpha, dotNH );
        return F * ( V * D );
    }
    vec2 DFGApprox( const in vec3 normal, const in vec3 viewDir, const in float roughness ) {
        float dotNV = saturate( dot( normal, viewDir ) );
        const vec4 c0 = vec4( - 1, - 0.0275, - 0.572, 0.022 );
        const vec4 c1 = vec4( 1, 0.0425, 1.04, - 0.04 );
        vec4 r = roughness * c0 + c1;
        float a004 = min( r.x * r.x, exp2( - 9.28 * dotNV ) ) * r.x + r.y;
        vec2 fab = vec2( - 1.04, 1.04 ) * a004 + r.zw;
        return fab;
    }
    vec3 EnvironmentBRDF( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float roughness ) {
        vec2 fab = DFGApprox( normal, viewDir, roughness );
        return specularColor * fab.x + specularF90 * fab.y;
    }
    void computeMultiscattering( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float roughness, inout vec3 singleScatter, inout vec3 multiScatter ) {
        vec2 fab = DFGApprox( normal, viewDir, roughness );
        vec3 FssEss = specularColor * fab.x + specularF90 * fab.y;
        float Ess = fab.x + fab.y;
        float Ems = 1.0 - Ess;
        vec3 Favg = specularColor + ( 1.0 - specularColor ) * 0.047619;
        vec3 Fms = FssEss * Favg / ( 1.0 - Ems * Favg );
        singleScatter += FssEss;
        multiScatter += Fms * Ems;
    }

    void RE_Direct_Physical( const in IncidentLight directLight, const in GeometricContext geometry, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
        float dotNL = saturate( dot( geometry.normal, directLight.direction ) );
        vec3 irradiance = dotNL * directLight.color;
        reflectedLight.directSpecular += irradiance * BRDF_GGX( directLight.direction, geometry.viewDir, geometry.normal, material.specularColor, material.specularF90, material.roughness );
        reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
    }
    void RE_IndirectDiffuse_Physical( const in vec3 irradiance, const in GeometricContext geometry, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
        reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
    }
    void RE_IndirectSpecular_Physical( const in vec3 radiance, const in vec3 irradiance, const in vec3 clearcoatRadiance, const in GeometricContext geometry, const in PhysicalMaterial material, inout ReflectedLight reflectedLight) {
        vec3 singleScattering = vec3( 0.0 );
        vec3 multiScattering = vec3( 0.0 );
        vec3 cosineWeightedIrradiance = irradiance * RECIPROCAL_PI;
        computeMultiscattering( geometry.normal, geometry.viewDir, material.specularColor, material.specularF90, material.roughness, singleScattering, multiScattering );
        vec3 diffuse = material.diffuseColor * ( 1.0 - ( singleScattering + multiScattering ) );
        reflectedLight.indirectSpecular += radiance * singleScattering;
        reflectedLight.indirectSpecular += multiScattering * cosineWeightedIrradiance;
        reflectedLight.indirectDiffuse += diffuse * cosineWeightedIrradiance;
    }
    #define cubeUV_maxMipLevel 8.0
    #define cubeUV_minMipLevel 4.0
    #define cubeUV_maxTileSize 256.0
    #define cubeUV_minTileSize 16.0
    float getFace( vec3 direction ) {
        vec3 absDirection = abs( direction );
        float face = - 1.0;
        if ( absDirection.x > absDirection.z ) {
            if ( absDirection.x > absDirection.y )
            face = direction.x > 0.0 ? 0.0 : 3.0;
            else
            face = direction.y > 0.0 ? 1.0 : 4.0;
        }
        else {
            if ( absDirection.z > absDirection.y )
            face = direction.z > 0.0 ? 2.0 : 5.0;
            else
            face = direction.y > 0.0 ? 1.0 : 4.0;
        }
        return face;
    }
    vec2 getUV( vec3 direction, float face ) {
        vec2 uv;
        if ( face == 0.0 ) {
            uv = vec2( direction.z, direction.y ) / abs( direction.x );
        }
        else if ( face == 1.0 ) {
            uv = vec2( - direction.x, - direction.z ) / abs( direction.y );
        }
        else if ( face == 2.0 ) {
            uv = vec2( - direction.x, direction.y ) / abs( direction.z );
        }
        else if ( face == 3.0 ) {
            uv = vec2( - direction.z, direction.y ) / abs( direction.x );
        }
        else if ( face == 4.0 ) {
            uv = vec2( - direction.x, direction.z ) / abs( direction.y );
        }
        else {
            uv = vec2( direction.x, direction.y ) / abs( direction.z );
        }
        return 0.5 * ( uv + 1.0 );
    }
    vec3 bilinearCubeUV( sampler2D envMap, vec3 direction, float mipInt ) {
        float face = getFace( direction );
        float filterInt = max( cubeUV_minMipLevel - mipInt, 0.0 );
        mipInt = max( mipInt, cubeUV_minMipLevel );
        float faceSize = exp2( mipInt );
        float texelSize = 1.0 / ( 3.0 * cubeUV_maxTileSize );
        vec2 uv = getUV( direction, face ) * ( faceSize - 1.0 );
        vec2 f = fract( uv );
        uv += 0.5 - f;
        if ( face > 2.0 ) {
            uv.y += faceSize;
            face -= 3.0;
        }
        uv.x += face * faceSize;
        if ( mipInt < cubeUV_maxMipLevel ) {
            uv.y += 2.0 * cubeUV_maxTileSize;
        }
        uv.y += filterInt * 2.0 * cubeUV_minTileSize;
        uv.x += 3.0 * max( 0.0, cubeUV_maxTileSize - 2.0 * faceSize );
        uv *= texelSize;
        vec3 tl = envMapTexelToLinear( texture( envMap, uv ) ).rgb;
        uv.x += texelSize;
        vec3 tr = envMapTexelToLinear( texture( envMap, uv ) ).rgb;
        uv.y += texelSize;
        vec3 br = envMapTexelToLinear( texture( envMap, uv ) ).rgb;
        uv.x -= texelSize;
        vec3 bl = envMapTexelToLinear( texture( envMap, uv ) ).rgb;
        vec3 tm = mix( tl, tr, f.x );
        vec3 bm = mix( bl, br, f.x );
        return mix( tm, bm, f.y );
    }
    #define r0 1.0
    #define v0 0.339
    #define m0 - 2.0
    #define r1 0.8
    #define v1 0.276
    #define m1 - 1.0
    #define r4 0.4
    #define v4 0.046
    #define m4 2.0
    #define r5 0.305
    #define v5 0.016
    #define m5 3.0
    #define r6 0.21
    #define v6 0.0038
    #define m6 4.0
    float roughnessToMip( float roughness ) {
        float mip = 0.0;
        if ( roughness >= r1 ) {
            mip = ( r0 - roughness ) * ( m1 - m0 ) / ( r0 - r1 ) + m0;
        }
        else if ( roughness >= r4 ) {
            mip = ( r1 - roughness ) * ( m4 - m1 ) / ( r1 - r4 ) + m1;
        }
        else if ( roughness >= r5 ) {
            mip = ( r4 - roughness ) * ( m5 - m4 ) / ( r4 - r5 ) + m4;
        }
        else if ( roughness >= r6 ) {
            mip = ( r5 - roughness ) * ( m6 - m5 ) / ( r5 - r6 ) + m5;
        }
        else {
            mip = - 2.0 * log2( 1.16 * roughness );
        }
        return mip;
    }
    vec4 textureCubeUV( sampler2D envMap, vec3 sampleDir, float roughness ) {
        float mip = clamp( roughnessToMip( roughness ), m0, cubeUV_maxMipLevel );
        float mipF = fract( mip );
        float mipInt = floor( mip );
        vec3 color0 = bilinearCubeUV( envMap, sampleDir, mipInt );
        if ( mipF == 0.0 ) {
            return vec4( color0, 1.0 );
        }
        else {
            vec3 color1 = bilinearCubeUV( envMap, sampleDir, mipInt + 1.0 );
            return vec4( mix( color0, color1, mipF ), 1.0 );
        }
    
    }
    
    const float envMapIntensity = 1.0;
    
    vec3 getIBLIrradiance( const in vec3 normal ) {
        vec3 worldNormal = inverseTransformDirection( normal, viewMatrix );
        vec4 envMapColor = textureCubeUV( envMap, worldNormal, 1.0 );
        return PI * envMapColor.rgb * envMapIntensity;
    }
    vec3 getIBLRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness ) {
        vec3 reflectVec;
        reflectVec = reflect( - viewDir, normal );
        reflectVec = normalize( mix( reflectVec, normal, roughness * roughness) );
        reflectVec = inverseTransformDirection( reflectVec, viewMatrix );
        vec4 envMapColor = textureCubeUV( envMap, reflectVec, roughness );
        return envMapColor.rgb * envMapIntensity;
    }

    vec3 ambientLightColor = vec3(0.0, 0.0, 0.0);
    
    vec3 shGetIrradianceAt( in vec3 normal, in vec3 shCoefficients[ 9 ] ) {
        float x = normal.x, y = normal.y, z = normal.z;
        vec3 result = shCoefficients[ 0 ] * 0.886227;
        result += shCoefficients[ 1 ] * 2.0 * 0.511664 * y;
        result += shCoefficients[ 2 ] * 2.0 * 0.511664 * z;
        result += shCoefficients[ 3 ] * 2.0 * 0.511664 * x;
        result += shCoefficients[ 4 ] * 2.0 * 0.429043 * x * y;
        result += shCoefficients[ 5 ] * 2.0 * 0.429043 * y * z;
        result += shCoefficients[ 6 ] * ( 0.743125 * z * z - 0.247708 );
        result += shCoefficients[ 7 ] * 2.0 * 0.429043 * x * z;
        result += shCoefficients[ 8 ] * 0.429043 * ( x * x - y * y );
        return result;
    }
    
    vec3 getAmbientLightIrradiance( const in vec3 ambientLightColor ) {
        vec3 irradiance = ambientLightColor;
        return irradiance;
    }
    #define RE_Direct				RE_Direct_Physical
    #define RE_IndirectDiffuse		RE_IndirectDiffuse_Physical
    #define RE_IndirectSpecular		RE_IndirectSpecular_Physical

    void main() {
        vec4 diffuseColor = vec4( diffuse, opacity );
        ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
        vec3 totalEmissiveRadiance = emissive;
        diffuseColor *= vColor;
        float roughnessFactor = roughness;
        float metalnessFactor = metalness;
        float faceDirection = gl_FrontFacing ? 1.0 : - 1.0;
        vec3 normal = normalize( vNormal );
        vec3 geometryNormal = normal;
        PhysicalMaterial material;
        material.diffuseColor = diffuseColor.rgb * ( 1.0 - metalnessFactor );
        vec3 dxy = max( abs( dFdx( geometryNormal ) ), abs( dFdy( geometryNormal ) ) );
        float geometryRoughness = max( max( dxy.x, dxy.y ), dxy.z );
        material.roughness = max( roughnessFactor, 0.0525 );
        material.roughness += geometryRoughness;
        material.roughness = min( material.roughness, 1.0 );
        material.specularColor = mix( vec3( 0.04 ), diffuseColor.rgb, metalnessFactor );
        material.specularF90 = 1.0;
        
        GeometricContext geometry;
        geometry.position = - vViewPosition;
        geometry.normal = normal;
        geometry.viewDir = ( isOrthographic ) ? vec3( 0, 0, 1 ) : normalize( vViewPosition );
        IncidentLight directLight;
        vec3 iblIrradiance = vec3( 0.0 );
        vec3 irradiance = getAmbientLightIrradiance( ambientLightColor );
        vec3 radiance = vec3( 0.0 );
        vec3 clearcoatRadiance = vec3( 0.0 );
        iblIrradiance += getIBLIrradiance( geometry.normal );
        radiance += getIBLRadiance( geometry.viewDir, geometry.normal, material.roughness );
        RE_IndirectDiffuse( irradiance, geometry, material, reflectedLight );
        RE_IndirectSpecular( radiance, iblIrradiance, clearcoatRadiance, geometry, material, reflectedLight );
        vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;
        vec3 totalSpecular = reflectedLight.directSpecular + reflectedLight.indirectSpecular;
        vec3 outgoingLight = totalDiffuse + totalSpecular + totalEmissiveRadiance;
        fragColor = vec4( outgoingLight, diffuseColor.a );
        fragColor = linearToOutputTexel( fragColor );
    }

#endif