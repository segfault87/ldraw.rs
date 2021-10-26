// Portions of shader code has been taken from Three.js source code.
//
// Copyright © 2010-2021 three.js authors

#version 300 es

precision mediump float;
precision highp float;
precision highp int;

#ifndef NUM_POINT_LIGHTS
    #define NUM_POINT_LIGHTS 0
#endif
#ifndef NUM_DIRECTIONAL_LIGHTS
    #define NUM_DIRECTIONAL_LIGHTS 0
#endif

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
    uniform vec3 specular;
    uniform float shininess;
    uniform float opacity;

    struct DirectionalLight {
        vec3 direction;
        vec3 color;
    };

    struct PointLight {
        vec3 position;
        vec3 color;
        float distance;
        float decay;
    };

    #if NUM_POINT_LIGHTS > 0
        uniform PointLight pointLights[NUM_POINT_LIGHTS];
    #endif
    #if NUM_DIRECTIONAL_LIGHTS > 0
        uniform DirectionalLight directionalLights[NUM_DIRECTIONAL_LIGHTS];
    #endif
    uniform vec3 ambientLightColor;
    uniform vec3 lightProbe[9];
    
    uniform mat4 viewMatrix;
    uniform vec3 cameraPosition;
    uniform bool isOrthographic;

    vec4 LinearTosRGB( in vec4 value ) {
        return vec4( mix( pow( value.rgb, vec3( 0.41666 ) ) * 1.055 - vec3( 0.055 ), value.rgb * 12.92, vec3( lessThanEqual( value.rgb, vec3( 0.0031308 ) ) ) ), value.a );
    }
    
    vec4 linearToOutputTexel( vec4 value ) {
        return LinearTosRGB( value );
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
    
    float G_BlinnPhong_Implicit( ) {
        return 0.25;
    }
    float D_BlinnPhong( const in float shininess, const in float dotNH ) {
        return RECIPROCAL_PI * ( shininess * 0.5 + 1.0 ) * pow( dotNH, shininess );
    }
    vec3 BRDF_BlinnPhong( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in vec3 specularColor, const in float shininess ) {
        vec3 halfDir = normalize( lightDir + viewDir );
        float dotNH = saturate( dot( normal, halfDir ) );
        float dotVH = saturate( dot( viewDir, halfDir ) );
        vec3 F = F_Schlick( specularColor, 1.0, dotVH );
        float G = G_BlinnPhong_Implicit( );
        float D = D_BlinnPhong( shininess, dotNH );
        return F * ( G * D );
    }

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
    vec3 getLightProbeIrradiance( const in vec3 lightProbe[ 9 ], const in vec3 normal ) {
        vec3 worldNormal = inverseTransformDirection( normal, viewMatrix );
        vec3 irradiance = shGetIrradianceAt( worldNormal, lightProbe );
        return irradiance;
    }
    vec3 getAmbientLightIrradiance( const in vec3 ambientLightColor ) {
        vec3 irradiance = ambientLightColor;
        return irradiance;
    }
    float getDistanceAttenuation( const in float lightDistance, const in float cutoffDistance, const in float decayExponent ) {
        if ( cutoffDistance > 0.0 && decayExponent > 0.0 ) {
            return pow( saturate( - lightDistance / cutoffDistance + 1.0 ), decayExponent );
        }
        return 1.0;
    }
    float getSpotAttenuation( const in float coneCosine, const in float penumbraCosine, const in float angleCosine ) {
        return smoothstep( coneCosine, penumbraCosine, angleCosine );
    }
    
    void getDirectionalLightInfo( const in DirectionalLight directionalLight, const in GeometricContext geometry, out IncidentLight light ) {
        light.color = directionalLight.color;
        light.direction = directionalLight.direction;
        light.visible = true;
    }
    
    void getPointLightInfo( const in PointLight pointLight, const in GeometricContext geometry, out IncidentLight light ) {
        vec3 lVector = pointLight.position - geometry.position;
        light.direction = normalize( lVector );
        float lightDistance = length( lVector );
        light.color = pointLight.color;
        light.color *= getDistanceAttenuation( lightDistance, pointLight.distance, pointLight.decay );
        light.visible = ( light.color != vec3( 0.0 ) );
    }

    struct BlinnPhongMaterial {
        vec3 diffuseColor;
        vec3 specularColor;
        float specularShininess;
        float specularStrength;
    };
    void RE_Direct_BlinnPhong( const in IncidentLight directLight, const in GeometricContext geometry, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
        float dotNL = saturate( dot( geometry.normal, directLight.direction ) );
        vec3 irradiance = dotNL * directLight.color;
        reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
        reflectedLight.directSpecular += irradiance * BRDF_BlinnPhong( directLight.direction, geometry.viewDir, geometry.normal, material.specularColor, material.specularShininess ) * material.specularStrength;
    }
    void RE_IndirectDiffuse_BlinnPhong( const in vec3 irradiance, const in GeometricContext geometry, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
        reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
    }
    #define RE_Direct				RE_Direct_BlinnPhong
    #define RE_IndirectDiffuse		RE_IndirectDiffuse_BlinnPhong

    void main() {
        vec4 diffuseColor = vec4( diffuse, opacity );
        ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
        vec3 totalEmissiveRadiance = emissive;
        diffuseColor *= vColor;
            
        float specularStrength;
        specularStrength = 1.0;
        float faceDirection = gl_FrontFacing ? 1.0 : - 1.0;
        vec3 normal = normalize( vNormal );
        vec3 geometryNormal = normal;

        BlinnPhongMaterial material;
        material.diffuseColor = diffuseColor.rgb;
        material.specularColor = specular;
        material.specularShininess = shininess;
        material.specularStrength = specularStrength;

        GeometricContext geometry;
        geometry.position = -vViewPosition;
        geometry.normal = normal;
        geometry.viewDir = ( isOrthographic ) ? vec3( 0, 0, 1 ) : normalize( vViewPosition );
        
        IncidentLight directLight;
        PointLight pointLight;
        
        #if NUM_POINT_LIGHTS > 0
            for (int i = 0; i < NUM_POINT_LIGHTS; ++i) {
                pointLight = pointLights[ i ];
                getPointLightInfo( pointLight, geometry, directLight );
                RE_Direct( directLight, geometry, material, reflectedLight );
            }
        #endif
        
        #if NUM_DIRECTIONAL_LIGHTS > 0
            for (int i = 0; i < NUM_DIRECTIONAL_LIGHTS; ++i) {
                DirectionalLight directionalLight;
                directionalLight = directionalLights[ 0 ];
                getDirectionalLightInfo( directionalLight, geometry, directLight );
                RE_Direct( directLight, geometry, material, reflectedLight );
            }
        #endif
        
        vec3 iblIrradiance = vec3( 0.0 );
        vec3 irradiance = getAmbientLightIrradiance( ambientLightColor );
        irradiance += getLightProbeIrradiance( lightProbe, geometry.normal );
        
        vec3 radiance = vec3( 0.0 );
        vec3 clearcoatRadiance = vec3( 0.0 );
        
        RE_IndirectDiffuse( irradiance, geometry, material, reflectedLight );
        
        vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + reflectedLight.directSpecular + reflectedLight.indirectSpecular + totalEmissiveRadiance;

        fragColor = vec4( outgoingLight, diffuseColor.a );
        fragColor = linearToOutputTexel( fragColor );
    }
#endif